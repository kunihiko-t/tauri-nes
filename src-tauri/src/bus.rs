use crate::ram::Memory;
use crate::cartridge::Cartridge;
use crate::ppu::Ppu;
use crate::cpu::Cpu6502;
use crate::controller::Controller;
use std::sync::{Arc, Mutex};
// Remove unused imports from cpu module
// use crate::cpu::{Registers, FLAG_BREAK, FLAG_INTERRUPT_DISABLE, FLAG_UNUSED};
use crate::cpu::InspectState;
use crate::ppu::FrameData;
// 存在しないモジュールのインポートを削除
// use crate::memory_access::MemoryAccess;
// TODO: Add references to PPU, APU, Cartridge, Controllers etc.

// The main system bus, connecting CPU, PPU, RAM, Cartridge, etc.
pub struct Bus {
    cpu_ram: Memory,
    ppu: Ppu,
    cpu: Cpu6502,
    cartridge: Option<Arc<Mutex<Cartridge>>>,
    pub controller1: Controller,
    pub total_cycles: u64,
    prev_nmi_line: bool,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            cpu_ram: Memory::new(),
            ppu: Ppu::new(),
            cpu: Cpu6502::new(),
            cartridge: None,
            controller1: Controller::new(),
            total_cycles: 0,
            prev_nmi_line: true,
        }
    }

    // Method to insert a cartridge into the bus
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(Arc::new(Mutex::new(cartridge)));
        self.reset(); // Reset system on cartridge insertion
    }

    // Read data from the bus at the specified address
    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let mirror_down_addr = addr & 0x07FF;
                let value = self.cpu_ram.ram[mirror_down_addr as usize];
                // Debug for RAM reads during reset
                if addr >= 0x0100 && addr <= 0x01FF {
                    println!("Stack read: ${:04X} = ${:02X}", addr, value);
                }
                value
            }
            0x2000..=0x3FFF => { // PPU Registers (+ mirrors)
                let register = addr & 0x0007;
                match register {
                    0x0002 /* PPUSTATUS */ => self.ppu.read_status(),
                    0x0004 /* OAMDATA */ => self.ppu.read_oam_data(),
                    0x0007 /* PPUDATA */ => {
                        // 1. Get current VRAM address from PPU
                        let vram_addr = self.ppu.get_vram_address();
                        // 2. Read data from VRAM/Palette via Bus helper
                        let data = if vram_addr >= 0x3F00 {
                            self.read_palette(vram_addr)
                        } else {
                            self.ppu_read_vram(vram_addr)
                        };
                        // 3. Let PPU update its internal read buffer
                        let result = self.ppu.handle_data_read_buffer(data);
                        // 4. Increment PPU VRAM address
                        self.ppu.increment_vram_addr();
                        result
                    }
                    _ => 0, // Read-only or unimplemented registers
                }
            }
            0x4000..=0x4015 => 0,  // APU registers (not implemented yet)
            0x4016 => self.controller1.read(), // Controller 1
            0x4017 => 0, // Controller 2 (not implemented yet)
            0x4018..=0x401F => 0, // Typically disabled APU/IO functionality
            0x4020..=0xFFFF => { // Cartridge space (PRG ROM, PRG RAM, Mapper registers)
                if let Some(cart) = &self.cartridge {
                    let value = cart.lock().unwrap().read_prg(addr);
                    // Debug for reset vector reads
                    if addr >= 0xFFFC && addr <= 0xFFFF {
                        println!("Vector read: ${:04X} = ${:02X}", addr, value);
                    }
                    value
                } else {
                    // カートリッジがロードされていない場合
                    if addr >= 0xFFFC && addr <= 0xFFFF {
                        // リセットベクタの範囲の場合、デフォルト値を返す
                        println!("No cartridge: Vector read ${:04X}, returning default 0", addr);
                    } else if addr >= 0x8000 {
                        // 他のPRG ROM領域の場合
                        println!("WARN: Read from PRG ROM at ${:04X} but no cartridge loaded", addr);
                    }
                    0xFF // カートリッジがない場合は0xFFを返す
                }
            }
        }
    }

    // Write data to the bus at the specified address
    pub fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram.write(addr & 0x07FF, data),
            0x2000..=0x3FFF => { // PPU Registers (+ mirrors)
                let register = addr & 0x0007;
                match register {
                    0x0000 /* PPUCTRL */ => self.ppu.write_ctrl(data),
                    0x0001 /* PPUMASK */ => self.ppu.write_mask(data),
                    0x0003 /* OAMADDR */ => self.ppu.write_oam_addr(data),
                    0x0004 /* OAMDATA */ => self.ppu.write_oam_data(data),
                    0x0005 /* PPUSCROLL */ => self.ppu.write_scroll(data),
                    0x0006 /* PPUADDR */ => self.ppu.write_addr(data),
                    0x0007 /* PPUDATA */ => {
                        // 1. Get current VRAM address from PPU
                        let vram_addr = self.ppu.get_vram_address();
                        // 2. Write data to VRAM/Palette via Bus helper
                        if vram_addr >= 0x3F00 {
                            self.write_palette(vram_addr, data);
                        } else {
                            self.ppu_write_vram(vram_addr, data);
                        }
                        // 3. Increment PPU VRAM address
                        self.ppu.increment_vram_addr();
                    }
                    _ => {} // Read-only or unimplemented registers
                }
            }
            0x4000..=0x4013 => {}, // APU registers (not implemented yet)
            0x4014 => {
                // OAM DMAトランスファー処理
                self.ppu.write_oam_dma(data);
                // 実際の処理はBus側で実装する
                self.trigger_oam_dma(data);
            },
            0x4015 => {}, // APU registers (not implemented yet)
            0x4016 => self.controller1.write(data), // Controller 1 Strobe
            0x4017 => {}, // TODO: Controller 2 / APU
            0x4018..=0x401F => {}, // Typically disabled APU/IO functionality
            0x4020..=0xFFFF => { // Cartridge space
                if let Some(cart) = &mut self.cartridge {
                    cart.lock().unwrap().write_prg(addr, data);
                }
            }
        }
    }

    // --- Palette RAM Access Helpers ---
    pub fn read_palette(&self, addr: u16) -> u8 {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // Mirror $3F1x to $3F0x
            _ => index,
        };
        self.ppu.palette_ram[mirrored_index]
    }

    pub fn write_palette(&mut self, addr: u16, data: u8) {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
             0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // Mirror $3F1x to $3F0x
            _ => index,
        };
        self.ppu.palette_ram[mirrored_index] = data & 0x3F; // Writes are masked to 6 bits
    }

    // --- PPU VRAM Access Helpers ---
    pub fn ppu_read(&mut self, addr: u16) -> u8 {
        if addr >= 0x3F00 {
            self.read_palette(addr)
        } else {
            self.ppu_read_vram(addr)
        }
    }
    
    pub fn ppu_write(&mut self, addr: u16, data: u8) {
        if addr >= 0x3F00 {
            self.write_palette(addr, data);
        } else {
            self.ppu_write_vram(addr, data);
        }
    }

    pub fn ppu_read_vram(&self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => { // CHR ROM/RAM
                self.cartridge.as_ref().map_or(0, |cart| cart.lock().unwrap().read_chr(addr))
            }
            0x2000..=0x3EFF => { // Nametable RAM (VRAM)
                if let Some(cart) = &self.cartridge {
                    let mirroring = cart.lock().unwrap().mirror_mode();
                    let mirrored_addr = self.ppu.mirror_vram_addr(addr, mirroring);
                    self.ppu.vram.get(mirrored_addr as usize).copied().unwrap_or(0)
                } else { 0 }
            }
            0x3F00..=0x3FFF => self.read_palette(addr), // Read Palette RAM
            _ => 0,
        }
    }

    pub fn ppu_write_vram(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => { // CHR RAM (if writable)
                if let Some(cart) = &mut self.cartridge {
                    cart.lock().unwrap().write_chr(addr, data);
                }
            }
            0x2000..=0x3EFF => { // Nametable RAM (VRAM)
                 if let Some(cart) = &self.cartridge {
                     let mirroring = cart.lock().unwrap().mirror_mode();
                     let mirrored_addr = self.ppu.mirror_vram_addr(addr, mirroring);
                     if mirrored_addr < self.ppu.vram.len() as u16 {
                        self.ppu.vram[mirrored_addr as usize] = data;
                     } else {
                        println!("Warning: Mirrored VRAM write out of bounds: {:04X} -> {:04X}", addr, mirrored_addr);
                     }
                 }
            }
            0x3F00..=0x3FFF => self.write_palette(addr, data), // Write to Palette RAM
            _ => {}
        }
    }

    // --- System Clocking (Reverted) ---
    pub fn clock(&mut self) -> u64 {
        let current_nmi_line_low = self.ppu.nmi_line_low;
        let nmi_edge_triggered = self.prev_nmi_line && !current_nmi_line_low;
        self.prev_nmi_line = current_nmi_line_low;

        let mut cpu_cycles: u8 = 0;
        
        if nmi_edge_triggered {
            // NMI処理を直接実装
            // 1. PCとステータスレジスタをスタックにプッシュ
            let pc = self.cpu.registers.program_counter;
            let status = self.cpu.registers.status;
            self.write(0x0100 + self.cpu.registers.stack_pointer as u16, (pc >> 8) as u8);
            self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
            self.write(0x0100 + self.cpu.registers.stack_pointer as u16, (pc & 0xFF) as u8);
            self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
            let status_for_push = (status & !crate::cpu::FLAG_BREAK) | crate::cpu::FLAG_UNUSED;
            self.write(0x0100 + self.cpu.registers.stack_pointer as u16, status_for_push);
            self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
            
            // 2. 割り込み禁止フラグを設定
            self.cpu.registers.status |= crate::cpu::FLAG_INTERRUPT_DISABLE;
            
            // 3. NMIベクタからPCをセット
            let lo = self.read(0xFFFA);
            let hi = self.read(0xFFFB);
            self.cpu.registers.program_counter = (hi as u16) << 8 | lo as u16;
            
            cpu_cycles = 7; // NMIは7サイクル消費
        } else {
            // 通常のCPU命令実行
            // このステップは`cpu.step(self)`に相当しますが、バス側で直接実装します
            // オペコードフェッチ、デコード、命令実行の基本形のみを実装
            let pc = self.cpu.registers.program_counter;
            
            // プログラムカウンタの値が無効な場合はリセット
            // 修正: ROMによっては$0000-$7FFFの領域にもコードが配置される場合があるため、実行を許可
            // 特にデモROMなどは非標準のメモリマッピングを使用することがある
            if pc == 0 {
                println!("Detected execution at zero address (${:04X}), resetting CPU", pc);
                self.reset();
                return 7; // 7サイクル消費と仮定
            }

            // オペコードをフェッチ
            let opcode = self.read(pc);
            
            // デバッグ出力（限定的に表示）
            if self.total_cycles % 1000 == 0 || pc >= 0xF000 || pc < 0x8000 {
                println!("CPU Execution: PC=${:04X}, opcode=${:02X} A=${:02X} X=${:02X} Y=${:02X} P=${:02X} SP=${:02X}", 
                    pc, opcode, self.cpu.registers.accumulator, self.cpu.registers.index_x, 
                    self.cpu.registers.index_y, self.cpu.registers.status, self.cpu.registers.stack_pointer);
            }
            
            self.cpu.registers.program_counter = pc.wrapping_add(1);
            
            // オペコードに基づいて命令を実行（簡易版）
            match opcode {
                0x00 => { // BRK
                    // 実行アドレスの履歴を保存
                    static mut BRK_HISTORY: [(u16, u32); 16] = [(0, 0); 16];
                    static mut HISTORY_INDEX: usize = 0;
                    static mut LOOP_DETECTED: bool = false;
                    
                    let pc = self.cpu.registers.program_counter.wrapping_add(1); // BRK comes with a padding byte
                    let status = self.cpu.registers.status;
                    
                    unsafe {
                        // 現在のBRKアドレスを履歴に記録
                        BRK_HISTORY[HISTORY_INDEX] = (pc, self.total_cycles as u32);
                        HISTORY_INDEX = (HISTORY_INDEX + 1) % 16;
                        
                        // 同じ範囲内でBRKが繰り返されていないか確認
                        let mut brk_in_range = 0;
                        let pc_range_start = (pc / 256) * 256; // 256バイト（1ページ）単位
                        
                        for &(hist_pc, _) in &BRK_HISTORY {
                            if hist_pc >= pc_range_start && hist_pc < pc_range_start + 256 {
                                brk_in_range += 1;
                            }
                        }
                        
                        // 同一ページ内で8回以上BRKが検出された場合はループとみなす
                        if brk_in_range >= 8 && !LOOP_DETECTED {
                            println!("BRK loop detected in page ${:02X}00, redirecting execution to $F000", pc_range_start >> 8);
                            LOOP_DETECTED = true;
                            
                            // スタックをリセット（RTIループを防止）
                            self.cpu.registers.stack_pointer = 0xFD;
                            
                            // 実行アドレスをリセットルーチン相当に設定
                            self.cpu.registers.program_counter = 0xF000;
                            return 2; // 短いサイクルで復帰
                        }
                        
                        // 一定間隔ごとにループ検出フラグをリセット
                        if self.total_cycles % 10000 == 0 {
                            LOOP_DETECTED = false;
                        }
                    }
                    
                    // デバッグ出力（簡略化）
                    if self.total_cycles % 1000 == 0 {
                        println!("BRK at ${:04X} -> IRQ/BRK vector", pc);
                    }
                    
                    // PCとステータスをスタックにプッシュ
                    self.write(0x0100 + self.cpu.registers.stack_pointer as u16, (pc >> 8) as u8);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
                    self.write(0x0100 + self.cpu.registers.stack_pointer as u16, (pc & 0xFF) as u8);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
                    
                    let status_with_break = status | crate::cpu::FLAG_BREAK | crate::cpu::FLAG_UNUSED;
                    self.write(0x0100 + self.cpu.registers.stack_pointer as u16, status_with_break);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
                    
                    // 割り込み禁止フラグを設定
                    self.cpu.registers.status |= crate::cpu::FLAG_INTERRUPT_DISABLE;
                    
                    // IRQ/BRKベクタからPCをセット
                    let lo = self.read(0xFFFE);
                    let hi = self.read(0xFFFF);
                    let vector = (hi as u16) << 8 | lo as u16;
                    
                    if self.total_cycles % 1000 == 0 {
                        println!("IRQ/BRK vector: ${:04X}", vector);
                    }
                    
                    self.cpu.registers.program_counter = vector;
                    cpu_cycles = 7;
                }
                0xEA => cpu_cycles = 2, // NOP: 2サイクル
                
                // JMP系命令
                0x4C => { // JMP Absolute (3 bytes, 3 cycles)
                    let lo = self.read(pc);
                    let hi = self.read(pc.wrapping_add(1));
                    self.cpu.registers.program_counter = (hi as u16) << 8 | lo as u16;
                    cpu_cycles = 3;
                }
                0x6C => { // JMP Indirect (3 bytes, 5 cycles)
                    let lo = self.read(pc);
                    let hi = self.read(pc.wrapping_add(1));
                    let addr = (hi as u16) << 8 | lo as u16;
                    
                    // 6502のバグを再現: ページ境界をまたぐ時はアドレスの下位バイトのみをインクリメント
                    let target_lo = self.read(addr);
                    let target_hi = if (addr & 0xFF) == 0xFF {
                        self.read(addr & 0xFF00)
                    } else {
                        self.read(addr.wrapping_add(1))
                    };
                    
                    self.cpu.registers.program_counter = (target_hi as u16) << 8 | target_lo as u16;
                    cpu_cycles = 5;
                }
                
                // JSR/RTS
                0x20 => { // JSR (3 bytes, 6 cycles)
                    // オペランドを正しい順序で読み取る
                    let lo = self.read(pc);
                    let hi = self.read(pc.wrapping_add(1));
                    
                    // デバッグ用：ジャンプ先アドレスを表示
                    let target_addr = ((hi as u16) << 8) | lo as u16;
                    println!("JSR - Jump to subroutine at ${:04X} (lo=${:02X}, hi=${:02X})", 
                             target_addr, lo, hi);
                    
                    // PCをスタックにプッシュ (PCはJSRの最後のバイトを指す)
                    let return_addr = pc.wrapping_add(1);
                    self.write(0x0100 + self.cpu.registers.stack_pointer as u16, (return_addr >> 8) as u8);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
                    self.write(0x0100 + self.cpu.registers.stack_pointer as u16, (return_addr & 0xFF) as u8);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_sub(1);
                    
                    // ジャンプ先をセット
                    self.cpu.registers.program_counter = target_addr;
                    
                    cpu_cycles = 6;
                }
                0x40 => { // RTI (1 byte, 6 cycles)
                    // 1. スタックからステータスレジスタを取得
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_add(1);
                    let status = self.read(0x0100 + self.cpu.registers.stack_pointer as u16);
                    // B フラグとbit 5（未使用）は無視される
                    self.cpu.registers.status = (status & !crate::cpu::FLAG_BREAK) | crate::cpu::FLAG_UNUSED;
                    
                    // 2. スタックからプログラムカウンタを取得
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_add(1);
                    let lo = self.read(0x0100 + self.cpu.registers.stack_pointer as u16);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_add(1);
                    let hi = self.read(0x0100 + self.cpu.registers.stack_pointer as u16);
                    
                    // PCを設定
                    let return_addr = ((hi as u16) << 8) | lo as u16;
                    
                    // デバッグ出力（簡略化）
                    if self.total_cycles % 1000 == 0 {
                        println!("RTI to ${:04X}", return_addr);
                    }
                    
                    self.cpu.registers.program_counter = return_addr;
                    cpu_cycles = 6;
                }
                
                // LDA系命令
                0xA9 => { // LDA Immediate (2 bytes, 2 cycles)
                    let value = self.read(pc);
                    self.cpu.registers.accumulator = value;
                    // Update N,Z flags
                    self.cpu.registers.status = if value == 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_ZERO
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_ZERO
                    };
                    self.cpu.registers.status = if value & 0x80 != 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_NEGATIVE
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_NEGATIVE
                    };
                    self.cpu.registers.program_counter = self.cpu.registers.program_counter.wrapping_add(1);
                    cpu_cycles = 2;
                }
                0xA5 => { // LDA Zero Page (2 bytes, 3 cycles)
                    let zp_addr = self.read(pc);
                    let value = self.read(zp_addr as u16);
                    self.cpu.registers.accumulator = value;
                    // Update N,Z flags
                    self.cpu.registers.status = if value == 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_ZERO
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_ZERO
                    };
                    self.cpu.registers.status = if value & 0x80 != 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_NEGATIVE
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_NEGATIVE
                    };
                    self.cpu.registers.program_counter = self.cpu.registers.program_counter.wrapping_add(1);
                    cpu_cycles = 3;
                }
                
                // STA系命令
                0x85 => { // STA Zero Page (2 bytes, 3 cycles)
                    let zp_addr = self.read(pc);
                    self.write(zp_addr as u16, self.cpu.registers.accumulator);
                    self.cpu.registers.program_counter = self.cpu.registers.program_counter.wrapping_add(1);
                    cpu_cycles = 3;
                }
                0x8D => { // STA Absolute (3 bytes, 4 cycles)
                    let lo = self.read(pc);
                    let hi = self.read(pc.wrapping_add(1));
                    let addr = (hi as u16) << 8 | lo as u16;
                    self.write(addr, self.cpu.registers.accumulator);
                    self.cpu.registers.program_counter = self.cpu.registers.program_counter.wrapping_add(2);
                    cpu_cycles = 4;
                }
                
                // レジスタ転送命令
                0xAA => { // TAX (1 byte, 2 cycles)
                    self.cpu.registers.index_x = self.cpu.registers.accumulator;
                    // Update N,Z flags
                    let value = self.cpu.registers.index_x;
                    self.cpu.registers.status = if value == 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_ZERO
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_ZERO
                    };
                    self.cpu.registers.status = if value & 0x80 != 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_NEGATIVE
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_NEGATIVE
                    };
                    cpu_cycles = 2;
                }
                0xA8 => { // TAY (1 byte, 2 cycles)
                    self.cpu.registers.index_y = self.cpu.registers.accumulator;
                    // Update N,Z flags
                    let value = self.cpu.registers.index_y;
                    self.cpu.registers.status = if value == 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_ZERO
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_ZERO
                    };
                    self.cpu.registers.status = if value & 0x80 != 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_NEGATIVE
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_NEGATIVE
                    };
                    cpu_cycles = 2;
                }
                
                // 追加命令の実装
                0x78 => { // SEI - 割り込み禁止フラグを設定
                    self.cpu.registers.status |= crate::cpu::FLAG_INTERRUPT_DISABLE;
                    cpu_cycles = 2;
                }
                
                0xD8 => { // CLD - 10進モード無効
                    self.cpu.registers.status &= !crate::cpu::FLAG_DECIMAL_MODE;
                    cpu_cycles = 2;
                }
                
                0xA2 => { // LDX Immediate
                    let value = self.read(pc);
                    self.cpu.registers.index_x = value;
                    // Update N,Z flags
                    self.cpu.registers.status = if value == 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_ZERO
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_ZERO
                    };
                    self.cpu.registers.status = if value & 0x80 != 0 {
                        self.cpu.registers.status | crate::cpu::FLAG_NEGATIVE
                    } else {
                        self.cpu.registers.status & !crate::cpu::FLAG_NEGATIVE
                    };
                    self.cpu.registers.program_counter = self.cpu.registers.program_counter.wrapping_add(1);
                    cpu_cycles = 2;
                }
                
                0x9A => { // TXS - Transfer X to Stack Pointer
                    self.cpu.registers.stack_pointer = self.cpu.registers.index_x;
                    // TXSは、N,Zフラグに影響しない
                    cpu_cycles = 2;
                }
                
                // 追加：RTSの実装を戻す
                0x60 => { // RTS (1 byte, 6 cycles)
                    // スタックからアドレスを取得
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_add(1);
                    let lo = self.read(0x0100 + self.cpu.registers.stack_pointer as u16);
                    self.cpu.registers.stack_pointer = self.cpu.registers.stack_pointer.wrapping_add(1);
                    let hi = self.read(0x0100 + self.cpu.registers.stack_pointer as u16);
                    
                    // PCをセット (RTSは1を足す必要がある)
                    let return_addr = ((hi as u16) << 8 | lo as u16).wrapping_add(1);
                    
                    // デバッグ出力（簡略化）
                    if self.total_cycles % 1000 == 0 {
                        println!("RTS to ${:04X}", return_addr);
                    }
                    
                    self.cpu.registers.program_counter = return_addr;
                    cpu_cycles = 6;
                },
                
                // FF - 多くのROMで見られる未定義オペコードの処理をNOPとして扱う
                0xFF => {
                    // FFはNOPとして扱い、プログラムカウンタを1つ進める（すでに上で実行済み）
                    println!("Encountered undefined opcode $FF at ${:04X}, treating as NOP", pc);
                    
                    // 連続してFFが出現した場合はリセットを検討
                    let ff_count = (0..10).map(|i| {
                        let addr = pc.wrapping_add(i);
                        self.debug_read(addr)
                    }).filter(|&op| op == 0xFF).count();
                    
                    if ff_count >= 8 {
                        println!("WARN: 多数の$FFオペコードが検出されました。リセットを実行します");
                        self.reset();
                        return 7;
                    }
                    
                    cpu_cycles = 2; // NOPと同様のサイクル数を消費
                },
                
                _ => {
                    // 実装されていないオペコード
                    if opcode != 0xFF && (pc < 0x8000 || pc >= 0xF000) {
                        // 特定の範囲のみログ出力
                        println!("Unimplemented opcode ${:02X} at ${:04X}, treating as NOP", opcode, pc);
                    }
                    
                    // ほとんどの未実装オペコードはNOPとして扱う
                    cpu_cycles = 2;
                }
            }
        }
        
        // PPUをクロック
        for _ in 0..(cpu_cycles * 3) {
            self.ppu.step();
        }
        
        self.total_cycles = self.total_cycles.wrapping_add(cpu_cycles as u64);
        cpu_cycles as u64
    }

    // --- Getters for inspection/frontend ---
    pub fn get_ppu_frame(&self) -> FrameData { self.ppu.get_frame() }
    pub fn get_cpu_state(&self) -> InspectState { self.cpu.inspect() }
    
    // --- PPU Access Methods ---
    pub fn write_ppu_mask(&mut self, value: u8) {
        self.ppu.write_mask(value);
    }
    
    pub fn is_frame_complete(&self) -> bool {
        self.ppu.frame_complete
    }
    
    pub fn reset_frame_complete(&mut self) {
        self.ppu.frame_complete = false;
    }

    // --- Debug Methods ---
    pub fn debug_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram.read(addr & 0x07FF),
            0x2000..=0x3FFF => {
                 let mirrored_addr = 0x2000 + (addr & 0x0007);
                 match mirrored_addr {
                     0x2002 => self.ppu.status.register,
                     0x2004 => self.ppu.oam_data[self.ppu.oam_addr as usize],
                     0x2007 => {
                         let vram_addr = self.ppu.vram_addr.get();
                         if vram_addr >= 0x3F00 {
                             self.read_palette(vram_addr)
                         } else {
                             self.ppu.data_buffer
                         }
                     }
                     _ => 0
                 }
            }
             0x4016 => 0,
             0x4017 => 0,
             0x4000..=0x4015 | 0x4018..=0x401F => 0,
             0x4020..=0xFFFF => {
                 self.cartridge.as_ref().map_or(0, |cart| cart.lock().unwrap().read_prg(addr))
             }
         }
    }

    pub fn monitor_zero_page_d2(&self) {
        let value = self.cpu_ram.read(0xD2);
        println!("ゼロページアドレス $D2 = {:02X} ({})", value, value);
    }

    pub fn trigger_oam_dma(&mut self, page: u8) {
        // ハイページのアドレスを計算 (e.g., $xx00)
        let base_addr = (page as u16) << 8;
        
        println!("OAM DMA initiated from page ${:02X}00", page);
        
        // 256バイト分のデータを転送
        for i in 0..256 {
            let addr = base_addr + i;
            let data = self.read(addr);
            self.ppu.oam_data[i as usize] = data;
        }
        
        // DMA転送は513または514サイクルかかる（奇数/偶数ページで異なる）
        // ここではCPUサイクルカウンタは使用していないので、実装を省略
    }

    pub fn debug_memory_dump(&self, start_addr: u16, length: u16) {
        println!("メモリダンプ - アドレス ${:04X}～${:04X}:", start_addr, start_addr + length - 1);
        
        for row in 0..(length + 15) / 16 {
            let row_start = start_addr + row * 16;
            let mut line = format!("${:04X}: ", row_start);
            
            for col in 0..16 {
                if row_start + col < start_addr + length {
                    let value = self.debug_read(row_start + col);
                    line.push_str(&format!("{:02X} ", value));
                } else {
                    line.push_str("   ");
                }
            }
            
            line.push_str(" | ");
            
            for col in 0..16 {
                if row_start + col < start_addr + length {
                    let value = self.debug_read(row_start + col);
                    if value >= 32 && value <= 126 {
                        line.push(value as char);
                    } else {
                        line.push('.');
                    }
                }
            }
            
            println!("{}", line);
            
            if row_start + 16 >= start_addr + length {
                break;
            }
        }
    }

    pub fn debug_disassemble(&self, start_addr: u16, num_instructions: u16) {
        let mut addr = start_addr;
        
        println!("ディスアセンブル - アドレス ${:04X}から{}命令:", start_addr, num_instructions);
        
        for _ in 0..num_instructions {
            let opcode = self.debug_read(addr);
            
            let (mnemonic, addr_mode) = match opcode {
                0xA9 => ("LDA", "imm"), 0xA5 => ("LDA", "zp"), 0xB5 => ("LDA", "zp,X"),
                0xAD => ("LDA", "abs"), 0xBD => ("LDA", "abs,X"), 0xB9 => ("LDA", "abs,Y"),
                0xA1 => ("LDA", "(ind,X)"), 0xB1 => ("LDA", "(ind),Y"),
                
                0x85 => ("STA", "zp"), 0x95 => ("STA", "zp,X"),
                0x8D => ("STA", "abs"), 0x9D => ("STA", "abs,X"), 0x99 => ("STA", "abs,Y"),
                0x81 => ("STA", "(ind,X)"), 0x91 => ("STA", "(ind),Y"),
                
                0xE8 => ("INX", "impl"), 0xC8 => ("INY", "impl"),
                0xCA => ("DEX", "impl"), 0x88 => ("DEY", "impl"),
                
                0xC9 => ("CMP", "imm"), 0xC5 => ("CMP", "zp"), 0xD5 => ("CMP", "zp,X"),
                0xCD => ("CMP", "abs"), 0xDD => ("CMP", "abs,X"), 0xD9 => ("CMP", "abs,Y"),
                0xC1 => ("CMP", "(ind,X)"), 0xD1 => ("CMP", "(ind),Y"),
                
                0xF0 => ("BEQ", "rel"), 0xD0 => ("BNE", "rel"),
                0xB0 => ("BCS", "rel"), 0x90 => ("BCC", "rel"),
                
                0x4C => ("JMP", "abs"), 0x6C => ("JMP", "(ind)"),
                0x20 => ("JSR", "abs"), 0x60 => ("RTS", "impl"),
                
                _ => {
                    let op_str = match opcode {
                        0x00 => "BRK",
                        0xEA => "NOP",
                        _ => "???",
                    };
                    (op_str, "")
                },
            };
            
            let bytes = match addr_mode {
                "impl" => 1,
                "imm" | "zp" | "zp,X" | "zp,Y" | "rel" | "(ind,X)" | "(ind),Y" => 2,
                "abs" | "abs,X" | "abs,Y" | "(ind)" => 3,
                _ => 1,
            };
            
            let operand1 = if bytes > 1 { self.debug_read(addr + 1) } else { 0 };
            let operand2 = if bytes > 2 { self.debug_read(addr + 2) } else { 0 };
            
            let mut instr = format!("${:04X}: {:02X} ", addr, opcode);
            if bytes > 1 { instr.push_str(&format!("{:02X} ", operand1)); } else { instr.push_str("   "); }
            if bytes > 2 { instr.push_str(&format!("{:02X} ", operand2)); } else { instr.push_str("   "); }
            
            match addr_mode {
                "impl" => instr.push_str(&format!("{}", mnemonic)),
                "imm" => instr.push_str(&format!("{} #${:02X}", mnemonic, operand1)),
                "zp" => instr.push_str(&format!("{} ${:02X}", mnemonic, operand1)),
                "zp,X" => instr.push_str(&format!("{} ${:02X},X", mnemonic, operand1)),
                "zp,Y" => instr.push_str(&format!("{} ${:02X},Y", mnemonic, operand1)),
                "rel" => {
                    let target = addr.wrapping_add(2).wrapping_add(
                        if operand1 & 0x80 != 0 { 
                            0xFF00u16 | operand1 as u16 
                        } else { 
                            operand1 as u16 
                        }
                    );
                    instr.push_str(&format!("{} ${:04X}", mnemonic, target));
                },
                "abs" => instr.push_str(&format!("{} ${:02X}{:02X}", mnemonic, operand2, operand1)),
                "abs,X" => instr.push_str(&format!("{} ${:02X}{:02X},X", mnemonic, operand2, operand1)),
                "abs,Y" => instr.push_str(&format!("{} ${:02X}{:02X},Y", mnemonic, operand2, operand1)),
                "(ind)" => instr.push_str(&format!("{} (${:02X}{:02X})", mnemonic, operand2, operand1)),
                "(ind,X)" => instr.push_str(&format!("{} (${:02X},X)", mnemonic, operand1)),
                "(ind),Y" => instr.push_str(&format!("{} (${:02X}),Y", mnemonic, operand1)),
                _ => instr.push_str(mnemonic),
            }
            
            println!("{}", instr);
            addr += bytes as u16;
        }
    }

    pub fn reset(&mut self) {
        println!("System Reset: Initializing CPU and PPU");
        
        // CPU RAMをクリア
        for i in 0..self.cpu_ram.ram.len() {
            self.cpu_ram.ram[i] = 0;
        }
        
        // リセットエラーフラグをクリア
        let mut reset_error = false;
        
        // リセットベクタを読み取る ($FFFC-$FFFD)
        let lo = self.read(0xFFFC);
        println!("Vector read: $FFFC = ${:02X}", lo);
        let hi = self.read(0xFFFD);
        println!("Vector read: $FFFD = ${:02X}", hi);
        
        // リセットベクタをアドレスに変換
        let reset_vector = (hi as u16) << 8 | lo as u16;
        println!("Reset vector read: $FFFC = ${:04X} (${:04X})", reset_vector, reset_vector);
        
        // 無効なリセットベクタの場合はデフォルト値を使用
        let pc = if reset_vector == 0 || reset_error {
            println!("Invalid reset vector, using default ($8000)");
            0x8000
        } else {
            reset_vector
        };
        
        // CPUレジスタをリセット
        self.cpu.registers.program_counter = pc;
        self.cpu.registers.stack_pointer = 0xFD;
        self.cpu.registers.accumulator = 0;
        self.cpu.registers.index_x = 0;
        self.cpu.registers.index_y = 0;
        self.cpu.registers.status = 0x24; // IRQ disable + unused bit
        
        // PPUをリセット
        self.ppu.reset();
        
        // PPUマスクレジスタを設定して描画を有効にする
        self.ppu.write_mask(0x1E); // 背景とスプライトを有効化
        
        // システム状態をリセット
        self.total_cycles = 0;
        self.prev_nmi_line = true;
        
        println!("--- System Reset Completed --- (PC set to ${:04X})", pc);
    }
}
