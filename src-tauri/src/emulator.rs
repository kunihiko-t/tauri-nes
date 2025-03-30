use crate::cpu::Cpu6502;
use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::NesRom;
use crate::ppu::{FrameData, Ppu};
// use std::sync::{Arc, Mutex}; // Removed unused import
// use std::path::Path; // Removed unused import
// TODO: Add other necessary components like PPU, APU, Cartridge

pub struct Emulator {
    cpu: Cpu6502,
    pub bus: Bus,
    ppu: Ppu,
    // apu: Apu,
    // cartridge: Option<Cartridge>, // Removed, Bus owns the cartridge now
    cycles_this_frame: u64,
    is_running: bool,
    // rom_path: Option<String>, // Keep track of loaded ROM path if needed
    // Add other state like running status, frame count etc.
}

impl Emulator {
    pub fn new() -> Self {
        // まずCPUとPPUを初期化します（Bus内のものとは別に）
        let cpu = Cpu6502::new();
        let ppu = Ppu::new();
        
        // 次にBusを初期化（Bus内部でも別のCPUとPPUインスタンスが作成される）
        let bus = Bus::new();

        Emulator {
            cpu,           // 外部CPUインスタンス
            bus,           // メインBus
            ppu,           // 外部PPUインスタンス
            cycles_this_frame: 0,
            is_running: true,
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
        // Correct program_counter access
        println!("CPU reset. PC starting at: {:#04X}", self.cpu.inspect().registers.program_counter);

        println!("ROM '{}' loaded successfully.", file_path);
        Ok(())
    }

    // Runs the emulator for approximately one frame
    pub fn run_frame(&mut self) -> u32 {
        // 1フレーム分のサイクルを実行
        let mut cycles_this_frame = 0u32;

        // 実行前のサイクル数を記録
        let start_cycles = self.bus.total_cycles;
        
        // フレーム完了フラグをリセット
        self.bus.reset_frame_complete();
        
        // フレームが完了するまでCPUサイクルを実行
        loop {
            // CPUクロックを実行
            let cycles = self.bus.clock();
            
            // サイクルカウンタを更新（オーバーフローを防止するためwrapping_addを使用）
            cycles_this_frame = cycles_this_frame.wrapping_add(cycles as u32);
            
            // フレーム完了チェック
            if self.bus.is_frame_complete() {
                // フレーム完了フラグをリセット
                self.bus.reset_frame_complete();
                break;
            }
            
            // 安全装置：過剰なサイクル数でループを抜ける（オーバーフロー対策）
            if cycles_this_frame > 100_000 {
                println!("WARNING: Safety break - excessive cycles: {}", cycles_this_frame);
                break;
            }
        }

        // 合計サイクル数を計算（オーバーフロー対策のためwrapping_sub使用）
        let total_executed = self.bus.total_cycles.wrapping_sub(start_cycles) as u32;
        println!("Frame complete at cycle: {}, cycles executed: {}", self.bus.total_cycles, total_executed);
        
        total_executed
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

    // Call the CPU's reset method via the Bus
    pub fn reset(&mut self) {
        self.bus.reset(); // Reset through the bus
        // Accessing PC needs adjustment due to InspectState structure change
        println!("CPU reset. PC starting at: {:#04X}", self.bus.get_cpu_state().registers.program_counter);
    }
}

// Default implementation for Emulator
impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}
