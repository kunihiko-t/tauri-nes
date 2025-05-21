// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri::Manager;
use tauri_nes::ppu::FrameData;
use tauri_nes::cpu::InspectState;
use tauri_nes::NesEmu;
use serde::Serialize;
use std::time::Instant;
use tauri_nes::bus::Bus;
use tauri_nes::cpu::Cpu6502;
use tauri_nes::cartridge::Cartridge;
use tauri_nes::emulator::Emulator;

// Define a struct to combine CPU state and PPU frame for frontend
#[derive(Serialize, Clone)]
struct FullStateInfo {
    cpu: InspectState,
    ppu_frame: FrameData,
}

// フレームを実行して取得するコマンド
#[tauri::command]
fn get_frame(state: tauri::State<'_, NesEmu>) -> Result<FrameData, String> {
    // First, check if ROM is loaded before doing anything else
    let rom_loaded = {
        let emulator_lock_check = state.emulator.lock();
        if let Ok(emu) = emulator_lock_check {
            emu.is_rom_loaded()
        } else {
            // If locking fails, assume no ROM is loaded or there's an issue
            // println!("Failed to lock emulator to check ROM status in get_frame command.");
            false // Treat lock failure as ROM not loaded for safety
        }
    };

    // If no ROM is loaded, return a default frame immediately without logging or processing
    if !rom_loaded {
        return Ok(FrameData::default());
    }

    // --- Proceed only if ROM is loaded ---
    static mut FRAME_COUNTER: u32 = 0;
    let start_time = std::time::Instant::now();
    
    let frame_number = unsafe {
        FRAME_COUNTER += 1;
        FRAME_COUNTER
    };
    
    // More detailed logging - Comment out
    // println!("Frame execution request #{} - start", frame_number);
    
    // Safely access the emulator field
    let mut emulator_lock = state.emulator.lock().map_err(|e| format!("Cannot lock emulator: {}", e))?;
    
    // Check test mode status via Bus
    let is_test_mode = emulator_lock.bus.test_mode;
    
    let result = if is_test_mode {
        // Comment out test mode log
        // println!("Test mode active, getting test frame for frame #{}", frame_number);
        emulator_lock.get_ppu_test_frame() // Call function to get test pattern
    } else {
        // Comment out normal mode log
        // println!("Normal mode, running frame #{} execution", frame_number);
        
        // Check initial CPU state (Only for normal execution)
        let bus = &emulator_lock.bus;
        let cpu_state = bus.get_cpu_state();
        // Comment out starting CPU state log
        // println!("Frame#{} starting CPU: PC=${:04X}, A=${:02X}, X=${:02X}, Y=${:02X}, SP=${:02X}",
        //          frame_number,
        //          cpu_state.registers.program_counter,
        //          cpu_state.registers.accumulator,
        //          cpu_state.registers.x_register,
        //          cpu_state.registers.y_register,
        //          cpu_state.registers.stack_pointer);
        
        // Run the frame
        let frame_result = emulator_lock.run_frame();
        
        if let Ok(frame) = &frame_result {
            // Output debug info
            let elapsed = start_time.elapsed();
            let non_zero = frame.pixels.iter().filter(|&&x| x != 0).count();
            // Comment out success log
            // println!("Frame#{} success: {}x{}, non-zero pixels: {}/{}, processing time: {:?}", 
            //          frame_number, frame.width, frame.height, non_zero, frame.pixels.len(), elapsed);
            
            // Report end CPU state
            let bus = &emulator_lock.bus;
            let cpu_state = bus.get_cpu_state();
            // Comment out end CPU state log
            // println!("Frame#{} end CPU: PC=${:04X}, A=${:02X}, X=${:02X}, Y=${:02X}, SP=${:02X}",
            //          frame_number,
            //          cpu_state.registers.program_counter,
            //          cpu_state.registers.accumulator,
            //          cpu_state.registers.x_register,
            //          cpu_state.registers.y_register,
            //          cpu_state.registers.stack_pointer);
        } else if let Err(e) = &frame_result {
            // Keep error log active
            println!("Frame execution error: {}", e);
        }
        
        frame_result
    };
    
    result
}

// キー入力イベントを処理するコマンド
#[tauri::command]
fn handle_key_event(state: tauri::State<'_, NesEmu>, key_code: String, pressed: bool) -> Result<(), String> {
    // Lock the emulator state
    let mut nes_emu = state.emulator.lock().map_err(|e| format!("Failed to lock emulator state: {}", e))?;
    
    // Call the consolidated key event handler in Emulator
    nes_emu.handle_key_event(&key_code, pressed);
    
    Ok(())
}

// ゲームROMをロードするコマンド
#[tauri::command]
fn load_rom(state: tauri::State<'_, NesEmu>, file_path: String) -> Result<bool, String> {
    println!("ROM load request: {}", file_path);
    
    // Lock the emulator
    let mut emulator = state.emulator.lock().map_err(|e| format!("Failed to lock emulator: {}", e))?;
    
    // Check if we need to toggle out of test mode
    if emulator.bus.test_mode {
        println!("Currently in test mode, toggling to normal mode before loading ROM");
        emulator.toggle_test_mode()?;
    }
    
    // Attempt to load the ROM
    match emulator.load_rom(&file_path) {
        Ok(_) => {
            println!("ROM loaded successfully: {}", file_path);
            
            // Check current CPU state after ROM load
            let cpu_state = emulator.bus.get_cpu_state();
            println!("CPU state after ROM load: PC=${:04X}, SP=${:02X}",
                    cpu_state.registers.program_counter,
                    cpu_state.registers.stack_pointer);
            
            // Check if ROM is flagged as loaded
            let is_rom_loaded = emulator.is_rom_loaded();
            println!("ROM loaded status: {}", is_rom_loaded);

            // Ensure reset happens right before returning OK
            println!("Performing final reset before returning from load_rom command...");
            emulator.bus.reset();

            Ok(true)
        },
        Err(err) => {
            println!("ROM load error: {}", err);
            Err(format!("Failed to load ROM: {}", err))
        }
    }
}

// Command to handle controller input (Deprecated)
#[tauri::command]
fn handle_input(
    input_data: tauri_nes::controller::InputData,
    emulator_state: State<'_, Arc<Mutex<Emulator>>>,
) {
    println!("Controller input received: {:?}", input_data);
    
    if let Ok(_emulator) = emulator_state.lock() {
        // Map key code based on button type
        let key_code = match input_data.button {
            tauri_nes::controller::Button::A => "KeyZ",
            tauri_nes::controller::Button::B => "KeyX",
            tauri_nes::controller::Button::Start => "Enter",
            tauri_nes::controller::Button::Select => "ShiftRight",
            tauri_nes::controller::Button::Up => "ArrowUp",
            tauri_nes::controller::Button::Down => "ArrowDown",
            tauri_nes::controller::Button::Left => "ArrowLeft",
            tauri_nes::controller::Button::Right => "ArrowRight",
        };
        
        // Pass input as key event to emulator
        println!("Converted to key input: {} (pressed: {})", key_code, input_data.pressed);
        // emulator.handle_input(key_code, input_data.pressed); // Error: handle_input removed, use handle_key_event
    } else {
        println!("Failed to lock emulator");
    }
}

// Tauri command to run the emulator for one frame and return FrameData (Deprecated)
#[tauri::command]
fn run_emulator_frame(state: State<'_, Arc<Mutex<Emulator>>>) -> Result<FrameData, String> {
    match state.lock() {
        Ok(mut emulator) => {
            // Run frame
            println!("Running emulator frame...");
            emulator.run_frame()
        },
        Err(e) => Err(format!("Failed to lock emulator state: {}", e))
    }
}

#[tauri::command]
fn get_cpu_state(state: tauri::State<Arc<Mutex<Emulator>>>) -> Result<InspectState, String> {
    let emulator = state.lock().map_err(|e| e.to_string())?;
    let cpu_state = emulator.bus.get_cpu_state();
    Ok(cpu_state)
}

// Add memory debug commands
#[tauri::command]
fn debug_memory(state: tauri::State<'_, Arc<Mutex<Emulator>>>, start_addr: u16, length: u16) -> Vec<u8> {
    let mut result = Vec::with_capacity(length as usize);
    if let Ok(emulator) = state.lock() {
        let bus = &emulator.bus;
        for addr in start_addr..(start_addr + length) {
            result.push(bus.debug_read(addr));
        }
        
        bus.debug_memory_dump(start_addr, length);
    }
    result
}

#[tauri::command]
fn debug_disassemble(state: tauri::State<'_, Arc<Mutex<Emulator>>>, start_addr: u16, num_instructions: u16) {
    if let Ok(emulator) = state.lock() {
        emulator.debug_disassemble_range(start_addr, num_instructions);
    }
}

#[tauri::command]
fn debug_zero_page(state: tauri::State<'_, Arc<Mutex<Emulator>>>) {
    if let Ok(emulator) = state.lock() {
        let bus = &emulator.bus;
        bus.debug_memory_dump(0x0000, 0x0100);
    }
}

#[tauri::command]
fn debug_stack(state: tauri::State<'_, Arc<Mutex<Emulator>>>) {
    if let Ok(emulator) = state.lock() {
        let bus = &emulator.bus;
        bus.debug_memory_dump(0x0100, 0x0100);
    }
}

#[tauri::command]
fn get_state_info(state: tauri::State<Arc<Mutex<Emulator>>>) -> Result<FullStateInfo, String> {
    let emulator_mutex = state.lock().map_err(|e| e.to_string())?;
    let bus = &emulator_mutex.bus;
    let cpu_state = bus.get_cpu_state();
    let frame_data = bus.get_ppu_frame();

    Ok(FullStateInfo {
        cpu: cpu_state,
        ppu_frame: frame_data,
    })
}

#[tauri::command]
fn debug_current_code(state: tauri::State<Arc<Mutex<Emulator>>>, num_instructions: u16) -> Result<(), String> {
    let emulator_mutex = state.lock().map_err(|e| e.to_string())?;
    let bus = &emulator_mutex.bus;
    let cpu_state = bus.get_cpu_state();
    let pc = cpu_state.registers.program_counter;
    
    // Get lock again and call method with mutable reference
    if let Ok(emulator) = state.lock() {
        emulator.debug_disassemble_range(pc, num_instructions);
    }
    
    Ok(())
}

#[tauri::command]
fn monitor_address(state: tauri::State<'_, Arc<Mutex<Emulator>>>, addr: u16) -> u8 {
    if let Ok(emulator) = state.lock() {
        let bus = &emulator.bus;
        bus.debug_read(addr)
    } else {
        0
    }
}

// Add key event handling method here (Deprecated)
#[tauri::command]
fn handle_keyboard_event(key_code: &str, pressed: bool, state: tauri::State<Arc<Mutex<Emulator>>>) -> Result<(), String> {
    match state.lock() {
        Ok(mut emulator) => {
            // Log key event information
            println!("Keyboard event received: key '{}' (pressed: {})", key_code, pressed);
            
            // Space key processing
            if key_code == "Space" {
                println!("Space key was {} - toggling test mode", if pressed { "pressed" } else { "released" });
                if pressed {
                    // Toggle test mode
                    emulator.toggle_test_mode();
                    println!("Test mode toggle completed");
                }
            } else {
                // Process other key inputs
                // emulator.handle_input(key_code, pressed); // Error: handle_input removed, use handle_key_event
            }
            
            Ok(())
        },
        Err(e) => Err(format!("Failed to lock emulator: {}", e))
    }
}

// Main entry point called from Tauri
fn main() {
    tauri::Builder::default()
        .manage(NesEmu {
            emulator: Arc::new(Mutex::new(Emulator::new())),
        })
        .invoke_handler(tauri::generate_handler![
            get_frame,
            handle_key_event,
            load_rom,
            // toggle_test_mode // Removed: This command is redundant, handled by handle_key_event
        ])
        .setup(|app| {
            let window = app.get_window("main").unwrap();
            window.set_title("Tauri NES Emulator").unwrap();
            window.show().unwrap();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Command to toggle test mode (Removed: Redundant)
/*
#[tauri::command]
fn toggle_test_mode(state: tauri::State<'_, NesEmu>) -> Result<(), String> {
    println!("Test mode toggle command received");
    
    let mut emulator = state.emulator.lock().map_err(|e| format!("Cannot lock emulator: {}", e))?;
    
    // Toggle test mode
    emulator.toggle_test_mode();
    println!("Test mode toggle completed");
    
    Ok(())
}
*/

#[tauri::command]
async fn run_single_frame(state: State<'_, NesEmu>) -> Result<Option<FrameData>, String> {
    let emulator_mutex = state.emulator.clone();
    let mut emulator = emulator_mutex.lock().map_err(|e| format!("Failed to lock emulator: {}", e))?;

    // Access test_mode field via bus
    if emulator.bus.test_mode {
        Ok(None)
    } else {
        let start_time = Instant::now();
        let start_cpu_state = emulator.bus.get_cpu_state();
        // println!(...); // Keep logs commented out

        // Call run_frame method instead of run_single_frame
        match emulator.run_frame() {
            Ok(frame_data) => {
                let duration = start_time.elapsed();
                let non_zero_pixels = frame_data.pixels.chunks_exact(3).filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0).count();
                let total_pixels = frame_data.width as usize * frame_data.height as usize * 3;
                // println!(...);
                let end_cpu_state = emulator.bus.get_cpu_state();
                // println!(...);
                Ok(Some(frame_data))
            },
            Err(e) => {
                eprintln!("Error running frame: {}", e);
                Err(format!("Error running frame: {}", e))
            }
        }
    }
}
