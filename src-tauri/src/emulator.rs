use crate::cpu::Cpu6502;
use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::NesRom;
use crate::ppu::{FrameData, Ppu};
// use std::fs; // Removed unused import
// use std::path::Path; // Removed unused import
// TODO: Add other necessary components like PPU, APU, Cartridge

pub struct Emulator {
    cpu: Cpu6502,
    bus: Bus,
    ppu: Ppu,
    // apu: Apu,
    // cartridge: Option<Cartridge>, // Removed, Bus owns the cartridge now
    cycles_this_frame: u64,
    // Add other state like running status, frame count etc.
}

impl Emulator {
    pub fn new() -> Self {
        let bus = Bus::new();
        let cpu = Cpu6502::new();
        let ppu = Ppu::new();

        // Reset the CPU. This reads the reset vector (0xFFFC) from the bus
        // to set the initial program counter. The bus needs to be mutable
        // here if reset involves writes (e.g., initializing PPU/APU registers),
        // but Cpu6502::reset currently only reads.
        // CPU reset will happen after loading ROM
        // cpu.reset(&mut bus); // Pass bus to reset

        Emulator {
            cpu,
            bus,
            ppu,
            // cartridge: None, // Removed
            cycles_this_frame: 0,
        }
    }

    // Load ROM from a file path
    pub fn load_rom(&mut self, file_path: &str) -> Result<(), String> {
        // 1. Use NesRom::from_file to read and parse the ROM directly
        println!("Attempting to load and parse ROM from: {}", file_path);
        let nes_rom = NesRom::from_file(file_path)
            .map_err(|e| format!("Failed to load or parse NES ROM '{}': {}", file_path, e))?;
        // println!("Successfully read {} bytes.", rom_data.len()); // Removed

        // 2. Parsing is done by NesRom::from_file
        // let nes_rom = NesRom::new(rom_data) // Removed
        //      .map_err(|e| format!("Failed to parse NES ROM '{}': {}", file_path, e))?;
        println!("Successfully parsed ROM header. Mapper: {}", nes_rom.mapper_id);

        // 3. Create Cartridge
        // Extract necessary fields from nes_rom
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
        println!("Cartridge created successfully.");

        // 4. Insert Cartridge into Bus
        //    This replaces the previous cartridge if any.
        self.bus.insert_cartridge(cartridge);
        println!("Cartridge inserted into bus.");

        // 5. Reset CPU to apply changes and read reset vector from cartridge
        self.cpu.reset(&mut self.bus);
        println!("CPU reset. PC starting at: {:#04X}", self.cpu.inspect().program_counter);

        println!("ROM '{}' loaded successfully.", file_path);
        Ok(())
    }

    // Runs the emulator for approximately one frame
    pub fn run_frame(&mut self) -> FrameData { // Return FrameData
        // Rough target cycles per frame (adjust as needed)
        // NTSC NES CPU runs at ~1.79 MHz, PPU at ~5.37 MHz (3x CPU)
        // Frame rate is ~60 Hz
        // CPU Cycles per frame = 1,789,773 / 60 ~= 29830
        const TARGET_CPU_CYCLES_PER_FRAME: u64 = 29830;

        let start_cycles = self.bus.total_cycles;
        let target_end_cycles = start_cycles + TARGET_CPU_CYCLES_PER_FRAME;

        // 各フレームの開始をログに記録
        println!("Starting new frame at cycle: {}", start_cycles);

        // 目標サイクル数に達するまでクロック
        while self.bus.total_cycles < target_end_cycles {
            // Busのclockメソッドを呼び出し、CPUとPPUの処理を任せる
            self.bus.clock();
        }

        println!("Frame complete at cycle: {}, cycles executed: {}", 
            self.bus.total_cycles, self.bus.total_cycles - start_cycles);

        // Return the rendered frame data
        self.ppu.get_frame()
    }

     // TODO: Implement handle_input method
     // pub fn handle_input(&mut self, input: controller::InputData) {
     //    // Update controller state on the bus
     // }

     // TODO: Implement inspect_cpu method (optional, for debugging)
     pub fn inspect_cpu(&self) -> crate::cpu::InspectState { // Make inspect public if needed
         self.cpu.inspect()
     }

     // メモリ内容のデバッグ表示を追加
     pub fn debug_memory(&self, start_addr: u16, count: u16) {
         println!("Memory dump from ${:04X} to ${:04X}:", start_addr, start_addr + count - 1);
         for i in 0..count {
             let addr = start_addr + i;
             let value = self.bus.debug_read(addr);
             print!("{:02X} ", value);
             if (i + 1) % 16 == 0 || i == count - 1 {
                 println!();
             }
         }
     }

    // デバッグ機能を追加
    pub fn debug_read(&self, addr: u16) -> u8 {
        self.bus.debug_read(addr)
    }

    pub fn debug_memory_dump(&self, start_addr: u16, length: u16) {
        self.bus.debug_memory_dump(start_addr, length)
    }

    pub fn debug_disassemble(&self, start_addr: u16, num_instructions: u16) {
        self.bus.debug_disassemble(start_addr, num_instructions)
    }
}

// Default implementation for Emulator
impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}
