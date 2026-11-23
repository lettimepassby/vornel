//! VNC 客户端（vnc-rs）：维护一份 RGBA 帧缓冲，原始图像与区域复制都在上面完成后推送脏矩形。

use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use vnc::{
    ClientKeyEvent, ClientMouseEvent, PixelFormat, VncConnector, VncEncoding, VncError, VncEvent,
    X11Event,
};

use crate::{encode_rect, keys, Event, Input, RemoteError, RemoteSession};

pub struct VncOptions {
    pub host: String,
    pub port: u16,
    pub password: String,
}

struct Framebuffer {
    w: u16,
    h: u16,
    data: Vec<u8>,
}

impl Framebuffer {
    fn new(w: u16, h: u16) -> Self {
        Self {
            w,
            h,
            data: vec![0; w as usize * h as usize * 4],
        }
    }

    fn stride(&self) -> usize {
        self.w as usize * 4
    }

    fn put(&mut self, x: u16, y: u16, w: u16, h: u16, pixels: &[u8]) {
        let stride = self.stride();
        for row in 0..h as usize {
            let yy = y as usize + row;
            if yy >= self.h as usize {
                break;
            }
            let cols = (w as usize).min((self.w as usize).saturating_sub(x as usize));
            let src = &pixels.get(row * w as usize * 4..(row * w as usize + cols) * 4);
            if let Some(src) = src {
                let dst = yy * stride + x as usize * 4;
                self.data[dst..dst + src.len()].copy_from_slice(src);
            }
        }
    }

    fn copy(&mut self, dst: vnc::Rect, src: vnc::Rect) {
        let stride = self.stride();
        let row_bytes = dst.width as usize * 4;
        let mut rows: Vec<Vec<u8>> = Vec::with_capacity(dst.height as usize);
        for row in 0..dst.height as usize {
            let s = (src.y as usize + row) * stride + src.x as usize * 4;
            match self.data.get(s..s + row_bytes) {
                Some(r) => rows.push(r.to_vec()),
                None => return,
            }
        }
        for (row, bytes) in rows.into_iter().enumerate() {
            let d = (dst.y as usize + row) * stride + dst.x as usize * 4;
            if let Some(target) = self.data.get_mut(d..d + row_bytes) {
                target.copy_from_slice(&bytes);
            }
        }
    }
}

fn vnc_err(e: VncError) -> RemoteError {
    RemoteError::Vnc(e.to_string())
}

pub async fn connect<F, E>(
    opts: VncOptions,
    mut on_frame: F,
    mut on_event: E,
) -> Result<RemoteSession, RemoteError>
where
    F: FnMut(Vec<u8>) + Send + 'static,
    E: FnMut(Event) + Send + 'static,
{
    let tcp = TcpStream::connect((opts.host.as_str(), opts.port)).await?;
    let password = opts.password.clone();
    let client = VncConnector::new(tcp)
        .set_auth_method(async move { Ok(password) })
        .add_encoding(VncEncoding::Tight)
        .add_encoding(VncEncoding::Zrle)
        .add_encoding(VncEncoding::CopyRect)
        .add_encoding(VncEncoding::Raw)
        .add_encoding(VncEncoding::DesktopSizePseudo)
        .allow_shared(true)
        .set_pixel_format(PixelFormat::rgba())
        .build()
        .map_err(vnc_err)?
        .try_start()
        .await
        .map_err(vnc_err)?
        .finish()
        .map_err(vnc_err)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Input>();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut fb = Framebuffer::new(0, 0);
        let mut buttons: u8 = 0;
        let mut ready = Some(ready_tx);
        let mut refresh = tokio::time::interval(std::time::Duration::from_millis(40));
        let reason = loop {
            tokio::select! {
                ev = client.poll_event() => match ev {
                    Ok(Some(VncEvent::SetResolution(s))) => {
                        fb = Framebuffer::new(s.width, s.height);
                        on_event(Event::Resize { width: s.width, height: s.height });
                        if let Some(r) = ready.take() {
                            let _ = r.send(());
                        }
                    }
                    Ok(Some(VncEvent::RawImage(r, data))) => {
                        fb.put(r.x, r.y, r.width, r.height, &data);
                        on_frame(encode_rect(&fb.data, fb.stride(), r.x, r.y, r.width, r.height));
                    }
                    Ok(Some(VncEvent::Copy(dst, src))) => {
                        fb.copy(dst, src);
                        on_frame(encode_rect(&fb.data, fb.stride(), dst.x, dst.y, dst.width, dst.height));
                    }
                    Ok(Some(VncEvent::Error(e))) => break Some(e),
                    Ok(Some(_)) => {}
                    Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
                    Err(e) => break Some(e.to_string()),
                },
                _ = refresh.tick() => {
                    // 定期请求增量更新
                    if client.input(X11Event::Refresh).await.is_err() {
                        break Some("连接已断开".into());
                    }
                }
                input = rx.recv() => match input {
                    None => {
                        let _ = client.close().await;
                        break None;
                    }
                    Some(Input::Pointer { x, y, button, down, moved }) => {
                        if !moved {
                            let bit = match button { 0 => 1, 1 => 2, 2 => 4, _ => 0 };
                            if down { buttons |= bit } else { buttons &= !bit }
                        }
                        let _ = client.input(X11Event::PointerEvent(ClientMouseEvent {
                            position_x: x, position_y: y, bottons: buttons,
                        })).await;
                    }
                    Some(Input::Wheel { x, y, delta }) => {
                        // 滚轮在 RFB 里是按钮 4（上）/ 5（下）的一次按下与松开
                        let bit = if delta > 0 { 8 } else { 16 };
                        for mask in [buttons | bit, buttons] {
                            let _ = client.input(X11Event::PointerEvent(ClientMouseEvent {
                                position_x: x, position_y: y, bottons: mask,
                            })).await;
                        }
                    }
                    Some(Input::Key { code, key, down }) => {
                        if let Some(sym) = keys::keysym(&code, &key) {
                            let _ = client.input(X11Event::KeyEvent(ClientKeyEvent { keycode: sym, down })).await;
                        }
                    }
                },
            }
        };
        on_event(Event::Closed(reason));
    });

    tokio::time::timeout(std::time::Duration::from_secs(20), ready_rx)
        .await
        .map_err(|_| RemoteError::Vnc("等待服务器画面超时".into()))?
        .map_err(|_| RemoteError::Vnc("连接已断开".into()))?;
    Ok(RemoteSession { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_put_and_copy() {
        let mut fb = Framebuffer::new(4, 2);
        fb.put(0, 0, 1, 1, &[9, 9, 9, 9]);
        fb.copy(
            vnc::Rect {
                x: 3,
                y: 1,
                width: 1,
                height: 1,
            },
            vnc::Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );
        let off = (fb.stride()) + 3 * 4;
        assert_eq!(&fb.data[off..off + 4], &[9, 9, 9, 9]);
    }
}
