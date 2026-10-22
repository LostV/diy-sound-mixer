//! Finding the user's chosen output device again after Windows or driver
//! updates. Endpoint ids change when a driver is reinstalled (NVIDIA HDMI
//! audio is notorious), and Windows may prefix names with a number:
//! `Headphones (A50 Game)` becomes `Headphones (2- A50 Game)`. The config
//! stores the name as well as the id, and the name is what counts.

use crate::{backend::DeviceInfo, config::OutputDevice};

/// Lowercased, whitespace collapsed, with Windows' `N- ` instance numbers removed.
pub fn device_key(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut rest = lower.as_str();
    while let Some(open) = rest.find('(') {
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with("- ") {
            rest = &rest[digits + 2..];
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The device matching `wanted`: same id, else same name, else same key.
pub fn find_device<'a>(devices: &'a [DeviceInfo], wanted: &OutputDevice) -> Option<&'a DeviceInfo> {
    if let Some(id) = &wanted.id
        && let Some(found) = devices.iter().find(|d| &d.id == id)
    {
        return Some(found);
    }
    devices.iter().find(|d| d.name == wanted.name).or_else(|| {
        let key = device_key(&wanted.name);
        devices.iter().find(|d| device_key(&d.name) == key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: name.into(),
        }
    }

    fn wanted(name: &str, id: Option<&str>) -> OutputDevice {
        OutputDevice {
            name: name.into(),
            id: id.map(str::to_string),
        }
    }

    #[test]
    fn key_strips_instance_numbers_and_case() {
        assert_eq!(
            device_key("Headphones (2- A50 Game)"),
            "headphones (a50 game)"
        );
        assert_eq!(device_key("Headphones (A50 Game)"), "headphones (a50 game)");
        assert_eq!(
            device_key("LG TV (10- NVIDIA High Definition Audio)"),
            "lg tv (nvidia high definition audio)"
        );
        assert_eq!(
            device_key("Speakers  (Realtek(R) Audio)"),
            "speakers (realtek(r) audio)"
        );
    }

    #[test]
    fn key_keeps_numbers_that_are_part_of_the_name() {
        assert_eq!(
            device_key("Speakers (5.1 Surround)"),
            "speakers (5.1 surround)"
        );
        assert_eq!(device_key("Monitor (2-Series)"), "monitor (2-series)");
    }

    #[test]
    fn finds_by_id_first() {
        let devices = [device("a", "Headset"), device("b", "Headset")];
        assert_eq!(
            find_device(&devices, &wanted("Headset", Some("b")))
                .unwrap()
                .id,
            "b"
        );
    }

    #[test]
    fn finds_by_name_when_id_changed_after_driver_update() {
        let devices = [device("new-id", "LG TV (NVIDIA High Definition Audio)")];
        let found = find_device(
            &devices,
            &wanted("LG TV (NVIDIA High Definition Audio)", Some("old-id")),
        );
        assert_eq!(found.unwrap().id, "new-id");
    }

    #[test]
    fn finds_renumbered_device() {
        let devices = [device("x", "Headphones (2- A50 Game)")];
        assert_eq!(
            find_device(&devices, &wanted("Headphones (A50 Game)", None))
                .unwrap()
                .id,
            "x"
        );
    }

    #[test]
    fn missing_device_is_none() {
        let devices = [device("x", "Speakers")];
        assert!(find_device(&devices, &wanted("Soundbar", None)).is_none());
    }
}
