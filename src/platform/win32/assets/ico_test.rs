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
