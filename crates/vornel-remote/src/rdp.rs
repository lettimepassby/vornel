//! RDP 客户端（IronRDP）：TLS + CredSSP/NLA 认证，按脏矩形推送画面。
//!
//! 每个会话运行在独立线程的单线程 tokio 运行时上，与其他会话互不影响。

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use ironrdp_connector::sspi::generator::NetworkRequest;
use ironrdp_connector::{
    ClientConnector, Config, ConnectionResult, ConnectorResult, Credentials, DesktopSize,
    ServerName,
};
use ironrdp_graphics::image_processing::PixelFormat;
use ironrdp_input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp_pdu::gcc::{ConnectionType, KeyboardType};
use ironrdp_pdu::rdp::capability_sets::{MajorPlatformType, RailSupportLevel};
use ironrdp_pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp_session::image::DecodedImage;
use ironrdp_session::{ActiveStageBuilder, ActiveStageOutput};
use ironrdp_tokio::{split_tokio_framed, FramedWrite, NetworkClient, TokioFramed};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

use crate::{encode_rect, keys, Event, Input, RemoteError, RemoteSession};

pub struct RdpOptions {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub domain: Option<String>,
    pub width: u16,
    pub height: u16,
}

/// 服务器 TLS 证书的公钥（用于“首次信任”校验）。
#[derive(Debug, Clone)]
pub struct ServerKey {
    pub fingerprint: String,
    /// base64
    pub public_key: String,
}

/// 只支持 NTLM；Kerberos 需要访问 KDC，这里明确拒绝。
struct NoKerberos;

impl NetworkClient for NoKerberos {
    #[allow(clippy::manual_async_fn)]
    fn send(
        &mut self,
        _request: &NetworkRequest,
    ) -> impl std::future::Future<Output = ConnectorResult<Vec<u8>>> {
        async {
            Err(ironrdp_connector::general_err!(
                "不支持 Kerberos 认证，请使用用户名和密码（NTLM）"
            ))
        }
    }
}

fn config(o: &RdpOptions) -> Config {
    Config {
        desktop_size: DesktopSize {
            width: o.width,
            height: o.height,
        },
        monitor_layout: None,
        desktop_scale_factor: 0,
        desktop_physical_size: None,
        desktop_orientation: None,
        device_scale_factor: None,
        monitor_extended_layout: None,
        relative_mouse_input: false,
        enable_tls: true,
        enable_credssp: true,
        enable_standard_rdp_security: false,
        credentials: Credentials::UsernamePassword {
            username: o.username.clone(),
            password: o.password.clone(),
        },
        domain: o.domain.clone(),
        client_build: 0,
        client_name: "vornel".to_owned(),
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: 0,
        connection_type: ConnectionType::Lan,
        ime_file_name: String::new(),
        bitmap: None,
        dig_product_id: String::new(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_owned(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        remote_application_mode: false,
        rail_support_level: RailSupportLevel::empty(),
        #[cfg(windows)]
        platform: MajorPlatformType::WINDOWS,
        #[cfg(target_os = "macos")]
        platform: MajorPlatformType::MACINTOSH,
        #[cfg(target_os = "ios")]
        platform: MajorPlatformType::IOS,
        #[cfg(target_os = "android")]
        platform: MajorPlatformType::ANDROID,
        #[cfg(not(any(windows, target_os = "macos", target_os = "ios", target_os = "android")))]
        platform: MajorPlatformType::UNIX,
        hardware_id: None,
        request_data: None,
        autologon: false,
        enable_audio_playback: false,
        enable_audio_capture: false,
        performance_flags: PerformanceFlags::default(),
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        compression_type: None,
        enable_server_pointer: false,
        pointer_software_rendering: true,
        multitransport_flags: None,
        support_dyn_vc_gfx_protocol: false,
    }
}

fn rdp_err(e: impl std::fmt::Display) -> RemoteError {
    RemoteError::Rdp(e.to_string())
}

type Upgraded = TokioFramed<ironrdp_tls::TlsStream<TcpStream>>;

async fn establish<V>(
    o: &RdpOptions,
    verify: V,
) -> Result<(Upgraded, ConnectionResult), RemoteError>
where
    V: FnOnce(&ServerKey) -> Result<(), String>,
{
    let tcp = TcpStream::connect((o.host.as_str(), o.port)).await?;
    let client_addr = tcp.local_addr()?;
    let mut framed = TokioFramed::new(tcp);
    let mut connector = ClientConnector::new(config(o), client_addr);
    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector)
        .await
        .map_err(rdp_err)?;

    let initial = framed.into_inner_no_leftover();
    let (tls, cert) = ironrdp_tls::upgrade(initial, &o.host).await?;
    let public_key = ironrdp_tls::extract_tls_server_public_key(&cert)
        .ok_or_else(|| RemoteError::Rdp("无法读取服务器证书的公钥".into()))?
        .to_vec();
    let key = ServerKey {
        fingerprint: format!(
            "SHA256:{}",
            STANDARD_NO_PAD.encode(Sha256::digest(&public_key))
        ),
        public_key: STANDARD.encode(&public_key),
    };
    verify(&key).map_err(RemoteError::Rejected)?;

    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let mut framed = TokioFramed::new(tls);
    let outcome = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut NoKerberos,
        ServerName::new(o.host.clone()),
        public_key,
        None,
    )
    .await
    .map_err(rdp_err)?;
    let result = outcome.into_connected().map_err(rdp_err)?;
    Ok((framed, result))
}

fn to_operations(input: Input) -> Vec<Operation> {
    match input {
        Input::Pointer {
            x,
            y,
            button,
            down,
            moved,
        } => {
            let pos = Operation::MouseMove(MousePosition { x, y });
            if moved {
                return vec![pos];
            }
            let btn = match button {
                0 => MouseButton::Left,
                1 => MouseButton::Middle,
                2 => MouseButton::Right,
                _ => return vec![pos],
            };
            vec![
                pos,
                if down {
                    Operation::MouseButtonPressed(btn)
                } else {
                    Operation::MouseButtonReleased(btn)
                },
            ]
        }
        Input::Wheel { x, y, delta } => vec![
            Operation::MouseMove(MousePosition { x, y }),
            Operation::WheelRotations(WheelRotations {
                is_vertical: true,
                rotation_units: delta,
            }),
        ],
        Input::Key { code, key, down } => {
            if let Some((extended, sc)) = keys::scancode(&code) {
                let sc = Scancode::from_u8(extended, sc);
                vec![if down {
                    Operation::KeyPressed(sc)
                } else {
                    Operation::KeyReleased(sc)
                }]
            } else {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => vec![if down {
                        Operation::UnicodeKeyPressed(c)
                    } else {
                        Operation::UnicodeKeyReleased(c)
                    }],
                    _ => Vec::new(),
                }
            }
        }
    }
}

async fn run<F, E>(
    framed: Upgraded,
    result: ConnectionResult,
    mut rx: mpsc::UnboundedReceiver<Input>,
    on_frame: &mut F,
    on_event: &mut E,
) -> Option<String>
where
    F: FnMut(Vec<u8>),
    E: FnMut(Event),
{
    let (mut reader, mut writer) = split_tokio_framed(framed);
    let size = result.desktop_size;
    on_event(Event::Resize {
        width: size.width,
        height: size.height,
    });
    let mut image = DecodedImage::new(PixelFormat::RgbA32, size.width, size.height);
    let mut stage = ActiveStageBuilder {
        static_channels: result.static_channels,
        user_channel_id: result.user_channel_id,
        io_channel_id: result.io_channel_id,
        message_channel_id: result.message_channel_id,
        share_id: result.share_id,
        compression_type: result.compression_type,
        enable_server_pointer: result.enable_server_pointer,
        pointer_software_rendering: result.pointer_software_rendering,
    }
    .build();
    let mut db = Database::new();

    loop {
        let outputs = tokio::select! {
            frame = reader.read_pdu() => match frame {
                Ok((action, payload)) => match stage.process(&mut image, action, &payload) {
                    Ok(o) => o,
                    Err(e) => return Some(e.to_string()),
                },
                Err(e) => return Some(format!("连接已断开：{e}")),
            },
            input = rx.recv() => match input {
                Some(i) => {
                    let events = db.apply(to_operations(i));
                    match stage.process_fastpath_input(&mut image, &events) {
                        Ok(o) => o,
                        Err(e) => return Some(e.to_string()),
                    }
                }
                None => {
                    // 界面关闭了会话：礼貌地断开
                    if let Ok(outputs) = stage.graceful_shutdown() {
                        for out in outputs {
                            if let ActiveStageOutput::ResponseFrame(f) = out {
                                let _ = writer.write_all(&f).await;
                            }
                        }
                    }
                    return None;
                }
            },
        };

        for out in outputs {
            match out {
                ActiveStageOutput::ResponseFrame(f) => {
                    if let Err(e) = writer.write_all(&f).await {
                        return Some(format!("发送失败：{e}"));
                    }
                }
                ActiveStageOutput::GraphicsUpdate(r) => {
                    let w = r.right.saturating_sub(r.left) + 1;
                    let h = r.bottom.saturating_sub(r.top) + 1;
                    on_frame(encode_rect(
                        image.data(),
                        image.stride(),
                        r.left,
                        r.top,
                        w,
                        h,
                    ));
                }
                ActiveStageOutput::Terminate(reason) => {
                    return Some(format!("远程会话已结束：{reason}"))
                }
                ActiveStageOutput::DeactivateAll => {
                    return Some("服务器要求重新激活会话（例如更改了分辨率），请重新连接".into())
                }
                _ => {}
            }
        }
    }
}

/// 连接 RDP 服务器。`verify` 在 TLS 握手后、发送凭据前调用，可拒绝未知或已变化的证书。
pub async fn connect<V, F, E>(
    opts: RdpOptions,
    verify: V,
    mut on_frame: F,
    mut on_event: E,
) -> Result<RemoteSession, RemoteError>
where
    V: FnOnce(&ServerKey) -> Result<(), String> + Send + 'static,
    F: FnMut(Vec<u8>) + Send + 'static,
    E: FnMut(Event) + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    std::thread::Builder::new()
        .name("vornel-rdp".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(RemoteError::Io(e)));
                    return;
                }
            };
            rt.block_on(async move {
                match establish(&opts, verify).await {
                    Ok((framed, result)) => {
                        let _ = ready_tx.send(Ok(()));
                        let reason = run(framed, result, rx, &mut on_frame, &mut on_event).await;
                        on_event(Event::Closed(reason));
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            });
        })?;
    ready_rx
        .await
        .map_err(|_| RemoteError::Rdp("连接线程意外退出".into()))??;
    Ok(RemoteSession { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_click_produces_move_and_press() {
        let ops = to_operations(Input::Pointer {
            x: 10,
            y: 20,
            button: 0,
            down: true,
            moved: false,
        });
        assert_eq!(ops.len(), 2);
        assert!(matches!(
            ops[1],
            Operation::MouseButtonPressed(MouseButton::Left)
        ));
    }

    #[test]
    fn unmapped_key_falls_back_to_unicode() {
        let ops = to_operations(Input::Key {
            code: "".into(),
            key: "é".into(),
            down: true,
        });
        assert!(matches!(ops[0], Operation::UnicodeKeyPressed('é')));
    }
}
