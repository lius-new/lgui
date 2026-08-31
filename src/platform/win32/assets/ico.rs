use std::io;

use windows::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, HICON};

pub(super) fn create_icon_from_ico_bytes(
    bytes: &[u8],
    width: i32,
    height: i32,
) -> io::Result<HICON> {
    let desired_size = width.max(height).max(1) as u32;
    let (offset, length) = pick_icon_image(bytes, desired_size)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid ICO data"))?;
    unsafe {
        CreateIconFromResourceEx(
            &bytes[offset..offset + length],
            true,
            0x0003_0000,
            width.max(1),
            height.max(1),
            Default::default(),
        )
    }
    .map_err(|error| io::Error::other(error.to_string()))
}

fn pick_icon_image(bytes: &[u8], desired_size: u32) -> Option<(usize, usize)> {
    if bytes.len() < 6 {
        return None;
    }

    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if bytes.len() < 6 + count * 16 {
        return None;
    }

    let mut best: Option<(u32, u32, usize, usize)> = None;
    for index in 0..count {
        let entry = 6 + index * 16;
        let width = if bytes[entry] == 0 {
            256
        } else {
            bytes[entry] as u32
        };
        let height = if bytes[entry + 1] == 0 {
            256
        } else {
            bytes[entry + 1] as u32
        };
        let edge = width.max(height);
        let length = u32::from_le_bytes([
            bytes[entry + 8],
            bytes[entry + 9],
            bytes[entry + 10],
            bytes[entry + 11],
        ]) as usize;
        let offset = u32::from_le_bytes([
            bytes[entry + 12],
            bytes[entry + 13],
            bytes[entry + 14],
            bytes[entry + 15],
        ]) as usize;
        if offset.checked_add(length)? > bytes.len() {
            continue;
        }

        let score = edge.abs_diff(desired_size);
        match best {
            Some((best_score, best_edge, _, _))
                if score > best_score || (score == best_score && edge <= best_edge) => {}
            _ => best = Some((score, edge, offset, length)),
        }
    }
    best.map(|(_, _, offset, length)| (offset, length))
}

#[cfg(test)]
mod tests {
    use super::pick_icon_image;

    #[test]
    fn icon_directory_selects_the_closest_larger_image_on_a_tie() {
        let mut bytes = vec![0_u8; 6 + 2 * 16 + 8];
        bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());
        bytes[6] = 16;
        bytes[7] = 16;
        bytes[14..18].copy_from_slice(&4_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&(38_u32).to_le_bytes());
        bytes[22] = 32;
        bytes[23] = 32;
        bytes[30..34].copy_from_slice(&4_u32.to_le_bytes());
        bytes[34..38].copy_from_slice(&(42_u32).to_le_bytes());

        assert_eq!(pick_icon_image(&bytes, 24), Some((42, 4)));
    }
}
