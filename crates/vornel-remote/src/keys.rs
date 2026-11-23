//! 键盘映射：浏览器的 `KeyboardEvent.code` → RDP 扫描码（PS/2 Set 1）；`key` → VNC 的 X11 keysym。

/// 返回 (是否扩展键, 扫描码)。
pub fn scancode(code: &str) -> Option<(bool, u8)> {
    let plain = |c: u8| Some((false, c));
    let ext = |c: u8| Some((true, c));
    if let Some(d) = code.strip_prefix("Digit") {
        let n: u8 = d.parse().ok()?;
        return plain(if n == 0 { 0x0B } else { 0x01 + n });
    }
    if let Some(f) = code.strip_prefix('F') {
        if let Ok(n) = f.parse::<u8>() {
            return match n {
                1..=10 => plain(0x3A + n),
                11 => plain(0x57),
                12 => plain(0x58),
                _ => None,
            };
        }
    }
    if let Some(k) = code.strip_prefix("Key") {
        let c = k.chars().next()?;
        const ROW: &[(char, u8)] = &[
            ('Q', 0x10),
            ('W', 0x11),
            ('E', 0x12),
            ('R', 0x13),
            ('T', 0x14),
            ('Y', 0x15),
            ('U', 0x16),
            ('I', 0x17),
            ('O', 0x18),
            ('P', 0x19),
            ('A', 0x1E),
            ('S', 0x1F),
            ('D', 0x20),
            ('F', 0x21),
            ('G', 0x22),
            ('H', 0x23),
            ('J', 0x24),
            ('K', 0x25),
            ('L', 0x26),
            ('Z', 0x2C),
            ('X', 0x2D),
            ('C', 0x2E),
            ('V', 0x2F),
            ('B', 0x30),
            ('N', 0x31),
            ('M', 0x32),
        ];
        return ROW
            .iter()
            .find(|(ch, _)| *ch == c)
            .map(|&(_, s)| (false, s));
    }
    match code {
        "Escape" => plain(0x01),
        "Minus" => plain(0x0C),
        "Equal" => plain(0x0D),
        "Backspace" => plain(0x0E),
        "Tab" => plain(0x0F),
        "BracketLeft" => plain(0x1A),
        "BracketRight" => plain(0x1B),
        "Enter" => plain(0x1C),
        "ControlLeft" => plain(0x1D),
        "Semicolon" => plain(0x27),
        "Quote" => plain(0x28),
        "Backquote" => plain(0x29),
        "ShiftLeft" => plain(0x2A),
        "Backslash" => plain(0x2B),
        "Comma" => plain(0x33),
        "Period" => plain(0x34),
        "Slash" => plain(0x35),
        "ShiftRight" => plain(0x36),
        "NumpadMultiply" => plain(0x37),
        "AltLeft" => plain(0x38),
        "Space" => plain(0x39),
        "CapsLock" => plain(0x3A),
        "NumLock" => plain(0x45),
        "ScrollLock" => plain(0x46),
        "Numpad7" => plain(0x47),
        "Numpad8" => plain(0x48),
        "Numpad9" => plain(0x49),
        "NumpadSubtract" => plain(0x4A),
        "Numpad4" => plain(0x4B),
        "Numpad5" => plain(0x4C),
        "Numpad6" => plain(0x4D),
        "NumpadAdd" => plain(0x4E),
        "Numpad1" => plain(0x4F),
        "Numpad2" => plain(0x50),
        "Numpad3" => plain(0x51),
        "Numpad0" => plain(0x52),
        "NumpadDecimal" => plain(0x53),
        "IntlBackslash" => plain(0x56),
        "NumpadEnter" => ext(0x1C),
        "ControlRight" => ext(0x1D),
        "NumpadDivide" => ext(0x35),
        "PrintScreen" => ext(0x37),
        "AltRight" => ext(0x38),
        "Home" => ext(0x47),
        "ArrowUp" => ext(0x48),
        "PageUp" => ext(0x49),
        "ArrowLeft" => ext(0x4B),
        "ArrowRight" => ext(0x4D),
        "End" => ext(0x4F),
        "ArrowDown" => ext(0x50),
        "PageDown" => ext(0x51),
        "Insert" => ext(0x52),
        "Delete" => ext(0x53),
        // macOS 的 ⌘ 映射为 Windows 键
        "MetaLeft" | "OSLeft" => ext(0x5B),
        "MetaRight" | "OSRight" => ext(0x5C),
        "ContextMenu" => ext(0x5D),
        _ => None,
    }
}

/// X11 keysym（RFC 6143 第 7.5.4 节）。
pub fn keysym(code: &str, key: &str) -> Option<u32> {
    let special = match key {
        "Backspace" => 0xff08,
        "Tab" => 0xff09,
        "Enter" => 0xff0d,
        "Escape" => 0xff1b,
        "Delete" => 0xffff,
        "Home" => 0xff50,
        "ArrowLeft" => 0xff51,
        "ArrowUp" => 0xff52,
        "ArrowRight" => 0xff53,
        "ArrowDown" => 0xff54,
        "PageUp" => 0xff55,
        "PageDown" => 0xff56,
        "End" => 0xff57,
        "Insert" => 0xff63,
        "CapsLock" => 0xffe5,
        "ContextMenu" => 0xff67,
        "Shift" => {
            if code == "ShiftRight" {
                0xffe2
            } else {
                0xffe1
            }
        }
        "Control" => {
            if code == "ControlRight" {
                0xffe4
            } else {
                0xffe3
            }
        }
        "Alt" => {
            if code == "AltRight" {
                0xffea
            } else {
                0xffe9
            }
        }
        "Meta" => {
            if code == "MetaRight" {
                0xffec
            } else {
                0xffeb
            }
        }
        _ => 0,
    };
    if special != 0 {
        return Some(special);
    }
    if let Some(n) = key.strip_prefix('F').and_then(|f| f.parse::<u32>().ok()) {
        if (1..=12).contains(&n) {
            return Some(0xffbe + n - 1);
        }
    }
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let cp = c as u32;
    // Latin-1 直接对应；其他 Unicode 字符用 0x01000000 + 码点
    Some(
        if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
            cp
        } else {
            0x0100_0000 + cp
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancodes() {
        assert_eq!(scancode("KeyA"), Some((false, 0x1E)));
        assert_eq!(scancode("Digit0"), Some((false, 0x0B)));
        assert_eq!(scancode("Digit1"), Some((false, 0x02)));
        assert_eq!(scancode("F1"), Some((false, 0x3B)));
        assert_eq!(scancode("F12"), Some((false, 0x58)));
        assert_eq!(scancode("ArrowUp"), Some((true, 0x48)));
        assert_eq!(scancode("Unknown"), None);
    }

    #[test]
    fn keysyms() {
        assert_eq!(keysym("KeyA", "a"), Some(0x61));
        assert_eq!(keysym("Enter", "Enter"), Some(0xff0d));
        assert_eq!(keysym("F5", "F5"), Some(0xffc2));
        assert_eq!(keysym("ShiftRight", "Shift"), Some(0xffe2));
        assert_eq!(keysym("", "中"), Some(0x0100_0000 + '中' as u32));
    }
}
