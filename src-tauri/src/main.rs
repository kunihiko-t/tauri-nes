// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex}; // Added for shared state
use tauri::State; // Added back for managed state
use crate::emulator::Emulator;
use crate::ppu::FrameData; // Import FrameData directly from ppu module
use crate::cpu::InspectState;  // InspectState構造体をインポート
use serde::Serialize;

mod cpu;
mod ram;
mod bus;
mod cartridge; // Added cartridge module
mod emulator;
mod ppu;
mod apu;
mod controller;
mod debugger;
mod registers;

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

    // Removed get_chr_rom as data is accessed directly or via Cartridge
}

// Define a struct to combine CPU state and PPU frame for frontend
#[derive(Serialize, Clone)]
struct FullStateInfo {
    cpu: InspectState,
    ppu_frame: FrameData,
}

// Command to load ROM into the emulator
#[tauri::command]
fn load_rom(
    file_path: String,
    emulator_state: State<'_, Arc<Mutex<emulator::Emulator>>>, // Access shared emulator state
) -> Result<(), String> { // Return Ok or Error string
    println!("Attempting to load ROM from: {}", file_path);
    // Lock the mutex to get mutable access to the Emulator
    let mut emulator = emulator_state.lock().map_err(|e| format!("Failed to lock emulator state: {}", e))?;
    // Call the emulator's load_rom method
    emulator.load_rom(&file_path)?; // Propagate error string, pass file_path as &str
    println!("ROM loaded successfully.");
    Ok(())
}

// Command to handle controller input
#[tauri::command]
fn handle_input(
    input_data: controller::InputData,
    emulator_state: State<'_, Arc<Mutex<emulator::Emulator>>>, // Access shared emulator state
) {
    // Lock the mutex to get mutable access to the Emulator
    let _emulator = emulator_state.lock().unwrap();
    // TODO: Implement handle_input method in Emulator struct
    // emulator.handle_input(input_data);
    println!("Input received: {:?}", input_data); // Placeholder
}

// Tauri command to run the emulator for one frame and return FrameData
#[tauri::command]
fn run_emulator_frame(state: State<'_, Arc<Mutex<Emulator>>>) -> Result<FrameData, String> {
    let mut emulator = state.lock().map_err(|e| e.to_string())?;
    Ok(emulator.run_frame())
}

#[tauri::command]
fn get_cpu_state(state: tauri::State<Arc<Mutex<Emulator>>>) -> Result<InspectState, String> {
    let emulator = state.lock().map_err(|e| e.to_string())?;
    // Access methods via emulator.bus
    let cpu_state = emulator.bus.get_cpu_state();
    // Remove unnecessary accesses to regs and pc within this function
    // let regs = cpu_state.registers;
    // let pc = regs.program_counter;

    // PPU Frame Data (not needed for this specific function, remove if unused)
    // let frame_data = emulator.bus.get_ppu_frame();

    Ok(cpu_state) // Return the fetched cpu_state
}

// メモリデバッグコマンドを追加
#[tauri::command]
fn debug_memory(state: tauri::State<'_, Arc<Mutex<Emulator>>>, start_addr: u16, length: u16) -> Vec<u8> {
    // メモリの内容を1バイトずつ配列で返す
    let mut result = Vec::with_capacity(length as usize);
    if let Ok(emulator) = state.lock() {
        for addr in start_addr..(start_addr + length) {
            result.push(emulator.debug_read(addr));
        }
        
        // コンソールにも表示
        emulator.debug_memory_dump(start_addr, length);
    }
    result
}

#[tauri::command]
fn debug_disassemble(state: tauri::State<'_, Arc<Mutex<Emulator>>>, start_addr: u16, num_instructions: u16) {
    if let Ok(emulator) = state.lock() {
        emulator.debug_disassemble(start_addr, num_instructions);
    }
}

#[tauri::command]
fn debug_zero_page(state: tauri::State<'_, Arc<Mutex<Emulator>>>) {
    if let Ok(emulator) = state.lock() {
        emulator.debug_memory_dump(0x0000, 0x0100);
    }
}

#[tauri::command]
fn debug_stack(state: tauri::State<'_, Arc<Mutex<Emulator>>>) {
    if let Ok(emulator) = state.lock() {
        emulator.debug_memory_dump(0x0100, 0x0100);
    }
}

#[tauri::command]
fn get_state_info(state: tauri::State<Arc<Mutex<Emulator>>>) -> Result<FullStateInfo, String> {
    let emulator_mutex = state.lock().map_err(|e| e.to_string())?;
    let cpu_state = emulator_mutex.bus.get_cpu_state(); // Access via emulator_mutex.bus
    let frame_data = emulator_mutex.bus.get_ppu_frame(); // Access via emulator_mutex.bus

    Ok(FullStateInfo {
        cpu: cpu_state,
        ppu_frame: frame_data,
    })
}

#[tauri::command]
fn debug_current_code(state: tauri::State<Arc<Mutex<Emulator>>>, num_instructions: u16) -> Result<(), String> {
    let emulator_mutex = state.lock().map_err(|e| e.to_string())?;
    let cpu_state = emulator_mutex.bus.get_cpu_state(); // Access via emulator_mutex.bus
    let pc = cpu_state.registers.program_counter;
    emulator_mutex.bus.debug_disassemble(pc, num_instructions); // Access via emulator_mutex.bus
    Ok(())
}

#[tauri::command]
fn monitor_address(state: tauri::State<'_, Arc<Mutex<Emulator>>>, addr: u16) -> u8 {
    if let Ok(emulator) = state.lock() {
        emulator.debug_read(addr)
    } else {
        0
    }
}

fn main() {
    // Create the Emulator instance and wrap it for safe sharing across threads
    let emulator = Arc::new(Mutex::new(emulator::Emulator::new()));

    // TODO: Add commands for stepping, running frame, getting framebuffer, getting CPU state etc.

    tauri::Builder::default()
        .manage(emulator) // Add the shared emulator state to Tauri
        .invoke_handler(tauri::generate_handler![
            load_rom,
            handle_input,
            run_emulator_frame, // Add the new command handler
            get_cpu_state,
            debug_memory,
            debug_disassemble,
            debug_zero_page,
            debug_stack,
            get_state_info,
            debug_current_code,
            monitor_address
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
