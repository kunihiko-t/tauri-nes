// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use tauri::State;

mod cpu;
mod ram;
mod ppu;
mod apu;
mod controller;
mod debugger;

const NES_HEADER_SIZE: usize = 16;
const PRG_ROM_PAGE_SIZE: usize = 16 * 1024;  // 16KB
const CHR_ROM_PAGE_SIZE: usize = 8 * 1024;   // 8KB


struct NesRom {
    chr_rom: Vec<u8>,
}

impl NesRom {
    fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // NESヘッダーの解析
        let prg_rom_size = buffer[4] as usize * PRG_ROM_PAGE_SIZE;
        let chr_rom_size = buffer[5] as usize * CHR_ROM_PAGE_SIZE;

        // CHR ROMの抽出
        let chr_rom_start = NES_HEADER_SIZE + prg_rom_size;
        let chr_rom = buffer[chr_rom_start..chr_rom_start + chr_rom_size].to_vec();

        Ok(NesRom { chr_rom })
    }

    // CHR ROMデータを返す
    fn get_chr_rom(&self) -> &[u8] {
        &self.chr_rom
    }
}

#[tauri::command]
fn send_chr_rom(file_path: String) -> Result<Vec<u8>, String> {
    let rom = match NesRom::from_file(file_path) {
        Ok(rom) => rom,
        Err(e) => return Err(format!("Failed to load ROM: {}", e)),
    };

    Ok(rom.get_chr_rom().to_vec())
}



#[tauri::command]
fn get_cpu_state(state: State<'_, cpu::CpuState>) -> Result<cpu::CpuState, String> {
    // 現在のCPUの状態を返す
    Ok(state.inner().clone())
}


#[tauri::command]
fn handle_input(input_data: controller::InputData) {
    // 入力データを処理する
}

fn main() {
    let mut memory = ram::Memory::new();
    let mut cpu = cpu::Cpu6502::new();
    cpu.run(&mut memory);

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![send_chr_rom, get_cpu_state, handle_input])
        .manage(cpu::CpuState::new())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
