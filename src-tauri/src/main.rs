// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri_nes::emulator::Emulator;
use tauri_nes::ppu::FrameData;
use tauri_nes::cpu::InspectState;
use tauri_nes::NesEmu;
use serde::Serialize;

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
    emulator_state: State<'_, Arc<Mutex<Emulator>>>,
) -> Result<(), String> {
    println!("Attempting to load ROM from: {}", file_path);
    let mut emulator = emulator_state.lock().map_err(|e| format!("Failed to lock emulator state: {}", e))?;
    emulator.load_rom(&file_path)?;
    println!("ROM loaded successfully.");
    Ok(())
}

// Command to handle controller input
#[tauri::command]
fn handle_input(
    input_data: tauri_nes::controller::InputData,
    emulator_state: State<'_, Arc<Mutex<Emulator>>>,
) {
    let _emulator = emulator_state.lock().unwrap();
    println!("Input received: {:?}", input_data);
}

// Tauri command to run the emulator for one frame and return FrameData
#[tauri::command]
fn run_emulator_frame(state: State<'_, Arc<Mutex<Emulator>>>) -> Result<FrameData, String> {
    let mut emulator = state.lock().map_err(|e| e.to_string())?;
    emulator.run_frame(); // フレームを実行
    Ok(emulator.bus.get_ppu_frame())
}

#[tauri::command]
fn get_cpu_state(state: tauri::State<Arc<Mutex<Emulator>>>) -> Result<InspectState, String> {
    let emulator = state.lock().map_err(|e| e.to_string())?;
    let cpu_state = emulator.bus.get_cpu_state();
    Ok(cpu_state)
}

// メモリデバッグコマンドを追加
#[tauri::command]
fn debug_memory(state: tauri::State<'_, Arc<Mutex<Emulator>>>, start_addr: u16, length: u16) -> Vec<u8> {
    let mut result = Vec::with_capacity(length as usize);
    if let Ok(emulator) = state.lock() {
        for addr in start_addr..(start_addr + length) {
            result.push(emulator.debug_read(addr));
        }
        
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
    let cpu_state = emulator_mutex.bus.get_cpu_state();
    let frame_data = emulator_mutex.bus.get_ppu_frame();

    Ok(FullStateInfo {
        cpu: cpu_state,
        ppu_frame: frame_data,
    })
}

#[tauri::command]
fn debug_current_code(state: tauri::State<Arc<Mutex<Emulator>>>, num_instructions: u16) -> Result<(), String> {
    let emulator_mutex = state.lock().map_err(|e| e.to_string())?;
    let cpu_state = emulator_mutex.bus.get_cpu_state();
    let pc = cpu_state.registers.program_counter;
    emulator_mutex.bus.debug_disassemble(pc, num_instructions);
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
    // NesEmuを使用
    let nes_emu = NesEmu::new();
    let emulator = nes_emu.emulator;

    tauri::Builder::default()
        .manage(emulator)
        .invoke_handler(tauri::generate_handler![
            load_rom,
            handle_input,
            run_emulator_frame,
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
