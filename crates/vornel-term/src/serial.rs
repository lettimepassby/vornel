//! 串口终端（桌面端）。iOS 没有串口，调用会返回“不支持”。

use crate::{TermError, TermSession};

#[derive(Debug, Clone)]
pub struct SerialOptions {
    pub path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    /// none / odd / even
    pub parity: String,
    pub stop_bits: u8,
    /// none / software / hardware
    pub flow_control: String,
}

/// 列出本机串口设备。
pub fn list_ports() -> Vec<String> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        serialport::available_ports()
            .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
            .unwrap_or_default()
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        Vec::new()
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn open<O, E>(
    opts: &SerialOptions,
    mut on_output: O,
    on_exit: E,
) -> Result<TermSession, TermError>
where
    O: FnMut(Vec<u8>) + Send + 'static,
    E: FnOnce(Option<String>) + Send + 'static,
{
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use crate::Cmd;

    let data_bits = match opts.data_bits {
        5 => DataBits::Five,
        6 => DataBits::Six,
        7 => DataBits::Seven,
        _ => DataBits::Eight,
    };
    let parity = match opts.parity.as_str() {
        "odd" => Parity::Odd,
        "even" => Parity::Even,
        _ => Parity::None,
    };
    let stop_bits = if opts.stop_bits == 2 {
        StopBits::Two
    } else {
        StopBits::One
    };
    let flow = match opts.flow_control.as_str() {
        "software" => FlowControl::Software,
        "hardware" => FlowControl::Hardware,
        _ => FlowControl::None,
    };

    let port = serialport::new(&opts.path, opts.baud_rate)
        .data_bits(data_bits)
        .parity(parity)
        .stop_bits(stop_bits)
        .flow_control(flow)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| TermError::Serial(e.to_string()))?;
    let mut reader = port
        .try_clone()
        .map_err(|e| TermError::Serial(e.to_string()))?;
    let mut writer = port;

    let stop = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Cmd>();

    // 读线程：串口读取是阻塞的，单独一个线程
    let stop_reader = stop.clone();
    std::thread::Builder::new()
        .name("vornel-serial-read".into())
        .spawn(move || {
            let mut buf = vec![0u8; 4096];
            let reason = loop {
                if stop_reader.load(Ordering::Relaxed) {
                    break None;
                }
                match reader.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => on_output(buf[..n].to_vec()),
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => break Some(format!("串口已断开：{e}")),
                }
            };
            stop_reader.store(true, Ordering::Relaxed);
            on_exit(reason);
        })?;

    // 写线程：接收界面输入
    std::thread::Builder::new()
        .name("vornel-serial-write".into())
        .spawn(move || {
            while let Some(cmd) = rx.blocking_recv() {
                if let Cmd::Write(d) = cmd {
                    if writer.write_all(&d).is_err() {
                        break;
                    }
                }
            }
            // 会话被关闭：通知读线程退出
            stop.store(true, Ordering::Relaxed);
        })?;

    Ok(TermSession { tx })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn open<O, E>(
    _opts: &SerialOptions,
    _on_output: O,
    _on_exit: E,
) -> Result<TermSession, TermError>
where
    O: FnMut(Vec<u8>) + Send + 'static,
    E: FnOnce(Option<String>) + Send + 'static,
{
    Err(TermError::Unsupported("此设备不支持串口".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_ports_does_not_panic() {
        let _ = list_ports();
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[test]
    fn opening_missing_port_fails_cleanly() {
        let opts = SerialOptions {
            path: "/dev/vornel-does-not-exist".into(),
            baud_rate: 115_200,
            data_bits: 8,
            parity: "none".into(),
            stop_bits: 1,
            flow_control: "none".into(),
        };
        assert!(open(&opts, |_| {}, |_| {}).is_err());
    }
}
