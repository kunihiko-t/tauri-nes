use crate::bus::Bus;
use crate::bus::BusAccess;
use crate::cartridge::Cartridge;
use crate::cpu::Cpu6502;
use crate::ppu::{FrameData, Ppu};
use crate::NesRom;
use std::sync::atomic::AtomicU32;
use std::println;

#[derive(Debug)]
pub enum EmulatorError {
    RomLoadError(String),
    InvalidRomFormat,
    StateError(String),
}

pub struct Emulator {
    // cpu: Cpu6502, // Remove direct ownership
    pub bus: Bus,
    // ppu: Ppu,     // Remove direct ownership
    cycles_this_frame: u64,
    pub is_running: bool,
    pub rom_loaded: bool,
    pub rom_path: Option<String>,
    brk_counter: u32, // BRK command counter
    frame_count: AtomicU32,
    test_mode: bool,
    frame_complete: bool,
    irq_cooldown: bool, // Add IRQ cooldown flag
}

impl Emulator {
    pub fn new() -> Self {
        let bus = Bus::new();
        
        Emulator {
            bus,
            cycles_this_frame: 0,
            is_running: false,
            rom_loaded: false,
            rom_path: None,
            brk_counter: 0, // BRK command counter
            frame_count: AtomicU32::new(0),
            test_mode: false,
            frame_complete: false,
            irq_cooldown: false, // Initialize IRQ cooldown
        }
    }

    pub fn load_rom(&mut self, file_path: &str) -> Result<(), String> {
        println!("ROM loading: {}", file_path);
        let nes_rom = NesRom::from_file(file_path)
            .map_err(|e| format!("ROM read error: {}", e))?;

        let prg_rom = nes_rom.prg_rom.clone(); // Clone data to pass ownership
        let chr_rom = nes_rom.chr_rom.clone();
        let mapper_id = nes_rom.mapper_id;
        let mirroring_flags = nes_rom.mirroring.into_flags(); // Get mirroring flags
        
        let cartridge = Cartridge::new(
            prg_rom,
            chr_rom,
            mapper_id,
            mirroring_flags,
        )?; // Propagate error from Cartridge::new
        
        {
            println!("Inserting cartridge into Bus");
            self.bus.insert_cartridge(cartridge);
            
            // Reset CPU
            println!("CPU/PPU reset");
            self.bus.reset();
            
            // Check CPU state after reset
            let cpu_state = self.bus.get_cpu_state();
            println!("ROM read after CPU state: PC=${:04X}, A=${:02X}, X=${:02X}, Y=${:02X}, SP=${:02X}, status=${:02X}",
                    cpu_state.registers.program_counter,
                    cpu_state.registers.accumulator,
                    cpu_state.registers.x_register,
                    cpu_state.registers.y_register,
                    cpu_state.registers.stack_pointer,
                    cpu_state.registers.status);
            
            if cpu_state.registers.stack_pointer != 0xFD {
                println!("Warning: ROM read after stack pointer is not correct: ${:02X}", 
                         cpu_state.registers.stack_pointer);
                self.bus.get_cpu_state_mut().registers.stack_pointer = 0xFD;
                println!("Stack pointer corrected to 0xFD");
            }
            
            // Check after test mode setting
            let cpu_state_after = self.bus.get_cpu_state();
            println!("CPU state after test mode setting: SP=${:02X}", 
                    cpu_state_after.registers.stack_pointer);
        }
        
        self.is_running = true;
        self.rom_loaded = true;
        self.rom_path = Some(file_path.to_string());
        Ok(())
    }

    pub fn handle_key_event(&mut self, key_code: &str, pressed: bool) {
        let btn = match key_code {
            "KeyZ" => Some(crate::controller::Button::A),
            "KeyX" => Some(crate::controller::Button::B),
            "ShiftRight" => Some(crate::controller::Button::Select),
            "Enter" => Some(crate::controller::Button::Start),
            "ArrowUp" => Some(crate::controller::Button::Up),
            "ArrowDown" => Some(crate::controller::Button::Down),
            "ArrowLeft" => Some(crate::controller::Button::Left),
            "ArrowRight" => Some(crate::controller::Button::Right),
            _ => None,
        };

        if let Some(btn) = btn {
            self.bus.controller1.borrow_mut().set_button_state(btn, pressed); // Use borrow_mut()
        }
    }

    pub fn run_frame(&mut self) -> Result<FrameData, String> {
        if !self.rom_loaded {
            return Ok(FrameData::default());
        }

        let max_cycles: u32 = 30000; // Prevent infinite loops
        let mut total_cycles: u32 = 0;
        let mut frame_complete = false;

        while !frame_complete && total_cycles < max_cycles {
            let step_cycles = {
                // Get raw pointer to bus
                let bus_ptr = &mut self.bus as *mut Bus;
                // Get mutable reference to CPU within the bus
                let mut cpu_ref = self.bus.cpu.borrow_mut();
                // Call step unsafely, passing the dereferenced bus pointer
                unsafe { cpu_ref.step(&mut *bus_ptr) as u32 }
            };
            total_cycles += step_cycles;

            // PPUをCPUサイクルの3倍ステップさせる
            for _ in 0..(step_cycles * 3) {
                self.bus.step_ppu();
            }

            // NMIチェック
            let current_nmi_line = self.bus.ppu.borrow().nmi_line_low;
            if !current_nmi_line && self.bus.prev_nmi_line { // Falling edge
                if self.bus.ppu.borrow().ctrl.generate_nmi() {
                    self.bus.cpu.borrow_mut().trigger_nmi();
                }
            }
            self.bus.prev_nmi_line = current_nmi_line; // Update previous state

            frame_complete = self.bus.is_frame_complete();
            if frame_complete {
                self.bus.reset_frame_complete();
                break;
            }
        }

        if total_cycles >= max_cycles {
            // println!("Frame execution limit reached: {} cycles", total_cycles);
        } else {
            // println!("Frame executed in {} cycles", total_cycles);
        }

        let frame = self.bus.get_ppu_frame();
        Ok(frame)
    }

    pub fn get_frame(&mut self) -> Result<FrameData, String> {
        if !self.rom_loaded {
            return Ok(FrameData::default());
        }

        let in_test_mode = self.bus.test_mode;

        if in_test_mode {
            // println!("Getting test frame...");
            return match self.bus.get_ppu_test_frame() {
                Ok(frame) => {
                    // println!("Test frame fetched: {}x{}", frame.width, frame.height);
                    let _non_zero = frame.pixels.iter().filter(|&p| *p != 0).count();
                    // println!("Test frame non-zero pixels: {}/{}", _non_zero, frame.pixels.len());
                    Ok(frame)
                },
                Err(e) => Err(format!("Failed to get test frame: {}", e)),
            };
        }

        let current_frame = self.frame_count.load(std::sync::atomic::Ordering::SeqCst);
        
        let result = self.execute_frame(current_frame as u64);
        
        self.frame_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        result
    }

    fn execute_frame(&mut self, frame_number: u64) -> Result<FrameData, String> {
        // println!("Frame execution request #{} - start", frame_number);

        // let start_cpu_state = self.bus.get_cpu_state();
        // println!("Frame#{} starting CPU: PC=${:04X}, A=${:02X}, X=${:02X}, Y=${:02X}, SP=${:02X}",
        //          frame_number,
        //          start_cpu_state.registers.program_counter,
        //          start_cpu_state.registers.accumulator,
        //          start_cpu_state.registers.x_register,
        //          start_cpu_state.registers.y_register,
        //          start_cpu_state.registers.stack_pointer);

        const TARGET_CYCLES_PER_FRAME: u64 = 29780; // NTSC
        let mut cycles_executed: u64 = 0;
        self.frame_complete = false;

        while cycles_executed < TARGET_CYCLES_PER_FRAME {
            let step_cycles = self.bus.clock(); // Bus::clock returns cycles executed by CPU

            cycles_executed += step_cycles;

            self.frame_complete = self.bus.is_frame_complete();
            if self.frame_complete {
                self.bus.reset_frame_complete();
                break;
            }
        }

        let frame_data = self.bus.get_ppu_frame_direct();
        let non_zero_pixels = frame_data.pixels.iter().filter(|&&p| p != 0).count();
        // println!("Frame execution success: cycles={}, non-zero pixels: {}/{}",
        //          cycles_executed,
        //          non_zero_pixels,
        //          frame_data.pixels.len());

        Ok(frame_data)
    }

    pub fn is_rom_loaded(&self) -> bool {
        self.bus.is_rom_loaded()
    }

    pub fn get_ppu_test_frame(&mut self) -> Result<FrameData, String> {
        self.bus.get_ppu_test_frame()
    }

    pub fn debug_disassemble_range(&self, start_addr: u16, num_instructions: u16) {
        let mut addr = start_addr;
        for _ in 0..num_instructions {
            let opcode = self.bus.read(addr);
            println!("${:04X}: {:02X}", addr, opcode);
            addr = addr.wrapping_add(1);
        }
    }

    pub fn reset(&mut self) {
        self.bus.reset();
        println!("CPU reset. PC starting at: {:#04X}", self.bus.get_cpu_state().registers.program_counter);
    }

    pub fn toggle_test_mode(&mut self) -> Result<(), String> {
        println!("Toggle test mode command received (Emulator wrapper)");
        self.bus.toggle_test_mode();
        let is_test_mode = self.bus.test_mode;
        println!("Test mode toggled via Emulator wrapper. New state: {}", if is_test_mode { "Enabled" } else { "Disabled" });
        Ok(())
    }

    pub fn print_debug_info(&self) {
        println!("=== Emulator Debug Information ===");
        let cpu_state = self.bus.get_cpu_state();
        println!("CPU: PC=${:04X}, A=${:02X}, X=${:02X}, Y=${:02X}, SP=${:02X}",
                cpu_state.registers.program_counter, cpu_state.registers.accumulator,
                cpu_state.registers.x_register, cpu_state.registers.y_register,
                cpu_state.registers.stack_pointer);
    }
}

// Default implementation for Emulator
impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}

