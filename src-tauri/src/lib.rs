use crate::emulator::Emulator;
use std::sync::{Arc, Mutex};
use std::io::{self, Read};
use std::fs::File;
use std::path::Path;

const NES_HEADER_SIZE: usize = 16;
const PRG_ROM_PAGE_SIZE: usize = 16 * 1024;  // 16KB
const CHR_ROM_PAGE_SIZE: usize = 8 * 1024;   // 8KB

// Enum for Nametable Mirroring types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Vertical,
    Horizontal,
    FourScreen,
    SingleScreenLower,
    SingleScreenUpper,
}

impl Mirroring {
    // Helper to convert Mirroring enum back to iNES flag bits (flags6)
    // Note: This is a simplified representation and might not cover all edge cases
    // or NES 2.0 specifics perfectly. SingleScreen modes are ambiguous here.
    pub fn into_flags(self) -> u8 {
        match self {
            Mirroring::Vertical => 0x01, // Bit 0 set for Vertical
            Mirroring::Horizontal => 0x00, // Bit 0 clear for Horizontal
            Mirroring::FourScreen => 0x08, // Bit 3 set for FourScreen
            // Map SingleScreen modes heuristically or based on common usage if needed
            Mirroring::SingleScreenLower => 0x00, // Treat as Horizontal for flag purposes?
            Mirroring::SingleScreenUpper => 0x01, // Treat as Vertical for flag purposes?
        }
    }
}

// Represents the parsed content of a .nes file header and data
#[derive(Debug, Clone)]
pub struct NesRom {
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub mapper_id: u8,
    pub mirroring: Mirroring,
    pub has_battery_backed_ram: bool,
    // pub prg_ram_size: usize, // Can be calculated or stored if needed
}

impl NesRom {
    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = File::open(path.as_ref())?; // Use as_ref()
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // TODO: Proper NES header validation and parsing
        if buffer.len() < NES_HEADER_SIZE || &buffer[0..4] != b"NES\x1a" {
             return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid NES ROM header"));
        }

        let prg_rom_pages = buffer[4] as usize;
        let chr_rom_pages = buffer[5] as usize;
        let flags6 = buffer[6];
        let flags7 = buffer[7];
        // TODO: Parse flags 8, 9, 10 for extended ROM sizes / NES 2.0 format

        let prg_rom_size = prg_rom_pages * PRG_ROM_PAGE_SIZE;
        let chr_rom_size = chr_rom_pages * CHR_ROM_PAGE_SIZE;

        let mapper_low = flags6 >> 4;
        let mapper_high = flags7 & 0xF0; // NES 2.0 uses flags7 upper nybble
        let mapper_id = mapper_high | mapper_low;

        let four_screen = (flags6 & 0x08) != 0;
        let vertical_mirroring = (flags6 & 0x01) != 0;
        let mirroring = match (four_screen, vertical_mirroring) {
            (true, _) => Mirroring::FourScreen,
            (false, true) => Mirroring::Vertical,
            (false, false) => Mirroring::Horizontal,
        };

        let has_battery_backed_ram = (flags6 & 0x02) != 0;

        // Determine if trainer is present (512 bytes before PRG ROM)
        let prg_rom_offset = NES_HEADER_SIZE + if (flags6 & 0x04) != 0 { 512 } else { 0 };

        if buffer.len() < prg_rom_offset + prg_rom_size + chr_rom_size {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ROM file size mismatch with header info"));
        }

        let prg_rom = buffer[prg_rom_offset..(prg_rom_offset + prg_rom_size)].to_vec();

        let chr_rom_offset = prg_rom_offset + prg_rom_size;
        let chr_rom = if chr_rom_size > 0 {
            buffer[chr_rom_offset..(chr_rom_offset + chr_rom_size)].to_vec()
        } else {
            // Use CHR RAM if CHR ROM size is 0
            // For now, just return empty vec, Cartridge/Mapper should handle CHR RAM allocation
            Vec::new()
        };


        Ok(NesRom {
            prg_rom,
            chr_rom,
            mapper_id,
            mirroring,
            has_battery_backed_ram,
        })
    }
}

// NESエミュレータのラッパーを定義
pub struct NesEmu {
    pub emulator: Arc<Mutex<Emulator>>,
}

impl NesEmu {
    pub fn new() -> Self {
        println!("NESエミュレータを初期化中...");
        let emulator = Arc::new(Mutex::new(Emulator::new()));
        
        // Emulatorを初期化しておく
        if let Ok(mut emu) = emulator.lock() {
            println!("初期テストパターンを描画中...");
            let _ = emu.toggle_test_mode(); // テストモードを有効化してパターンを描画
        } else {
            println!("エミュレータのロックに失敗しました");
        }
        
        NesEmu { emulator }
    }
}

// 他のモジュールをエクスポート
pub mod cpu;
pub mod ram;
pub mod bus;
pub mod cartridge;
pub mod emulator;
pub mod ppu;
pub mod apu;
pub mod controller;
pub mod debugger;
pub mod registers;

// Tauriコマンド実装はsrc-tauri/src/main.rsに移動
// このファイルからは削除しました

// 重複するget_frame関数定義を削除
// #[tauri::command]
// pub fn get_frame(nes: State<'_, NesEmu>) -> Result<FrameData, String> {
//     println!("フレーム取得リクエスト - タウリ関数実行");
//     let start_time = std::time::Instant::now();
//     
//     let mut nes = nes.inner.lock().map_err(|e| e.to_string())?;
//     let emulator = nes.emulator.as_mut().ok_or("エミュレータが初期化されていません")?;
//     
//     // フレーム実行を試行
//     match emulator.run_frame() {
//         Ok(frame) => {
//             // デバッグ情報の出力
//             let elapsed = start_time.elapsed();
//             let non_zero = frame.pixels.iter().filter(|&&x| x != 0).count();
//             println!("フレーム実行成功: {}x{}, 非ゼロピクセル: {}/{}, 処理時間: {:?}", 
//                      frame.width, frame.height, non_zero, frame.pixels.len(), elapsed);
//             
//             Ok(frame)
//         },
//         Err(e) => {
//             println!("フレーム実行エラー: {}", e);
//             Err(format!("フレーム実行エラー: {}", e))
//         }
//     }
// } 