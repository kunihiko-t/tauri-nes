use serde::Serialize;

#[derive(Serialize)]
pub struct Memory {
    pub ram: Vec<u8>,
// 2KBのRAM
    // その他のメモリ領域（PPUレジスタ、APU/I/Oレジスタ、ROM等）も追加
}

impl Memory {
    pub(crate) fn new() -> Self {
        Self {
            ram: vec![0; 2048],
            // その他の領域の初期化
        }
    }

    pub(crate) fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x07FF => self.ram[addr as usize],
            0x0800..=0x1FFF => self.ram[(addr % 2048) as usize], // ミラー
            // その他のアドレス範囲に対する読み込み処理
            _ => 0,
        }
    }

    pub(crate) fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x07FF => self.ram[addr as usize] = data,
            0x0800..=0x1FFF => self.ram[(addr % 2048) as usize] = data, // ミラー
            // その他のアドレス範囲に対する書き込み処理
            _ => {},
        }
    }
}
