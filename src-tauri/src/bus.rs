use crate::ram::Memory;
use crate::cartridge::Cartridge;
use crate::ppu::Ppu;
use crate::cpu::Cpu6502;
use crate::controller::Controller;
// TODO: Add references to PPU, APU, Cartridge, Controllers etc.

// The main system bus, connecting CPU, PPU, RAM, Cartridge, etc.
pub struct Bus {
    cpu_ram: Memory,
    ppu: Ppu,
    cpu: Cpu6502,
    cartridge: Option<Box<Cartridge>>,
    pub controller1: Controller,
    pub total_cycles: u64,
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
        }
    }

    // Method to insert a cartridge into the bus
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(Box::new(cartridge));
        // Temporarily take CPU to call reset, then put it back
        let mut cpu = std::mem::take(&mut self.cpu);
        cpu.reset(self);
        self.cpu = cpu;
    }

    // Read data from the bus at the specified address
    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // CPU RAM (mirrored every 0x800 bytes)
                self.cpu_ram.read(addr & 0x07FF)
            }
            0x2000..=0x3FFF => {
                let mirrored_addr = 0x2000 + (addr & 0x0007);
                match mirrored_addr {
                    0x2002 => { // PPUSTATUS ($2002) Read
                        // Save the current status before modifying it
                        let status = self.ppu.status;
                        
                        // Reading PPUSTATUS has side effects:
                        // 1. Clear the VBlank flag (bit 7)
                        self.ppu.status &= !0x80;
                        // 2. Reset the address latch
                        self.ppu.address_latch_low = true;
                        
                        // Return the status we saved
                        status
                    }
                    0x2004 => { // OAMDATA ($2004) Read
                        // Simple OAM read using current OAM address
                        self.ppu.oam_data[self.ppu.oam_addr as usize]
                    }
                    0x2007 => { // PPUDATA ($2007) Read
                        let addr = self.ppu.vram_addr.get(); // Get current VRAM address
                        let vram_increment = if (self.ppu.ctrl & 0x04) == 0 { 1 } else { 32 };
                        let result: u8;

                        if addr >= 0x3F00 { // Palette RAM
                            result = self.read_palette(addr); // Direct read for palettes
                            // Buffer still gets updated with VRAM data underneath
                            self.ppu.data_buffer = self.ppu_read_vram(addr & 0x2FFF);
                        } else { // VRAM
                            result = self.ppu.data_buffer; // Return the buffered data
                            self.ppu.data_buffer = self.ppu_read_vram(addr); // Fill buffer with new data
                        }

                        // Increment VRAM address
                        self.ppu.vram_addr.increment(vram_increment);
                        result
                    }
                    _ => 0, // Other PPU registers are write-only
                }
            }
            0x4016 => self.controller1.read(),
            0x4017 => 0, // TODO: Controller 2
            0x4000..=0x4015 | 0x4018..=0x401F => 0, // TODO: APU/IO
            0x4020..=0xFFFF => {
                match &self.cartridge {
                    Some(cart) => cart.read_prg(addr),
                    None => 0,
                }
            }
        }
    }

    // Write data to the bus at the specified address
    pub fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.cpu_ram.write(addr & 0x07FF, data);
            }
            0x2000..=0x3FFF => {
                let mirrored_addr = 0x2000 + (addr & 0x0007);
                match mirrored_addr {
                    0x2000 => { // PPUCTRL ($2000) Write
                        let old_nmi_output = self.ppu.nmi_output;
                        self.ppu.ctrl = data;
                        self.ppu.nmi_output = (data & 0x80) != 0; // Update NMI output flag
                        
                        // If NMI output went from disabled to enabled and VBlank is already set,
                        // trigger an NMI immediately
                        if !old_nmi_output && self.ppu.nmi_output && (self.ppu.status & 0x80) != 0 {
                            self.ppu.nmi_occurred = true;
                        }
                        
                        // Update temporary VRAM address nametable select
                        self.ppu.temp_vram_addr.set_nametable_select(data & 0x03);
                    }
                    0x2001 => { // PPUMASK ($2001) Write
                        self.ppu.mask = data;
                    }
                    0x2003 => { // OAMADDR ($2003) Write
                        self.ppu.oam_addr = data;
                    }
                    0x2004 => { // OAMDATA ($2004) Write
                        self.ppu.oam_data[self.ppu.oam_addr as usize] = data;
                        self.ppu.oam_addr = self.ppu.oam_addr.wrapping_add(1);
                    }
                    0x2005 => { // PPUSCROLL ($2005) Write
                        if self.ppu.address_latch_low { // First write (X scroll)
                            self.ppu.temp_vram_addr.set_coarse_x(data >> 3);
                            self.ppu.fine_x_scroll = data & 0x07;
                            self.ppu.address_latch_low = false;
                        } else { // Second write (Y scroll)
                            self.ppu.temp_vram_addr.set_coarse_y(data >> 3);
                            self.ppu.temp_vram_addr.set_fine_y(data & 0x07);
                            self.ppu.address_latch_low = true;
                        }
                    }
                    0x2006 => { // PPUADDR ($2006) Write
                        if self.ppu.address_latch_low { // First write (high byte)
                            // Clear upper byte and set bits 8-13 from data
                            self.ppu.temp_vram_addr.address = 
                                (self.ppu.temp_vram_addr.address & 0x00FF) | 
                                (((data & 0x3F) as u16) << 8);
                            self.ppu.address_latch_low = false;
                        } else { // Second write (low byte)
                            // Clear lower byte and set bits 0-7 from data
                            self.ppu.temp_vram_addr.address = 
                                (self.ppu.temp_vram_addr.address & 0xFF00) | 
                                (data as u16);
                            // Copy temp to actual VRAM address
                            self.ppu.vram_addr = self.ppu.temp_vram_addr;
                            self.ppu.address_latch_low = true;
                        }
                    }
                    0x2007 => { // PPUDATA ($2007) Write
                        let addr = self.ppu.vram_addr.get(); // Get current VRAM address
                        let vram_increment = if (self.ppu.ctrl & 0x04) == 0 { 1 } else { 32 };
                        
                        if addr >= 0x3F00 { // Palette RAM
                            self.write_palette(addr, data);
                        } else { // VRAM
                            self.ppu_write_vram(addr, data);
                        }
                        
                        // Increment VRAM address
                        self.ppu.vram_addr.increment(vram_increment);
                    }
                    _ => {}, // $2002 is read-only
                }
            }
            0x4014 => self.trigger_oam_dma(data),
            0x4016 => self.controller1.write(data),
            0x4017 => {}, // TODO: Controller 2 / APU
            0x4000..=0x4013 | 0x4015..=0x401F => {}, // TODO: APU/IO
            0x8000..=0xFFFF => {
                if let Some(cart) = &mut self.cartridge {
                    cart.write_prg(addr, data);
                }
            }
            _ => {},
        };
    }

    // --- Palette RAM Access Helpers --- (Keep these, PPU methods will call them)
    pub fn read_palette(&self, addr: u16) -> u8 {
        let index = (addr & 0x1F) as usize;
        // Handle mirroring for reads ($3F10/$3F14/$3F18/$3F1C mirror $3F00/$3F04/$3F08/$3F0C)
        let mirrored_index = match index {
            0x10 => 0x00,
            0x14 => 0x04,
            0x18 => 0x08,
            0x1C => 0x0C,
            _ => index,
        };
        // Read from PPU's palette RAM, return full byte (masking usually happens on write)
        self.ppu.palette_ram[mirrored_index]
    }

    pub fn write_palette(&mut self, addr: u16, data: u8) {
        let index = (addr & 0x1F) as usize;
         // Handle mirroring for writes ($3F10/$3F14/$3F18/$3F1C mirror $3F00/$3F04/$3F08/$3F0C)
        let mirrored_index = match index {
            0x10 => 0x00,
            0x14 => 0x04,
            0x18 => 0x08,
            0x1C => 0x0C,
            _ => index,
        };
        // Write to PPU's palette RAM, masking data to 6 bits for color
        self.ppu.palette_ram[mirrored_index] = data & 0x3F; // NES colors are 6-bit
    }

    // --- PPU VRAM Access Helpers --- (Simplify - PPU handles mirroring)
    pub fn ppu_read_vram(&self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF; // Ensure address is within PPU range
        
        // アドレス空間に応じて読み込み先を振り分け
        match addr {
            0x0000..=0x1FFF => {
                // パターンテーブル（CHR ROM/RAM）からの読み込み
                match &self.cartridge {
                    Some(cart) => cart.read_chr(addr),
                    None => 0, // カートリッジがない場合は0を返す
                }
            },
            0x2000..=0x3EFF => {
                // ネームテーブル（VRAM）からの読み込み
                if let Some(cart) = &self.cartridge {
                    let mirroring = cart.mirror_mode();
                    let mirrored_addr = self.ppu.mirror_vram_addr(addr, mirroring);
                    // PPUの内部VRAMから読み込み
                    if mirrored_addr < self.ppu.vram.len() as u16 {
                        self.ppu.vram[mirrored_addr as usize]
                    } else {
                        0 // 範囲外アクセス
                    }
                } else {
                    0 // カートリッジがない場合
                }
            },
            0x3F00..=0x3FFF => {
                // パレットRAMからの読み込み
                self.read_palette(addr)
            },
            _ => 0, // 不明なアドレス空間
        }
    }

    pub fn ppu_write_vram(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x3FFF;
        
        // アドレス空間に応じて書き込み先を振り分け
        match addr {
            0x0000..=0x1FFF => {
                // パターンテーブル（CHR ROM/RAM）への書き込み - マッパーにより可能性
                if let Some(cart) = &mut self.cartridge {
                    cart.write_chr(addr, data);
                }
            },
            0x2000..=0x3EFF => {
                // ネームテーブル（VRAM）への書き込み
                if let Some(cart) = &self.cartridge {
                    let mirroring = cart.mirror_mode();
                    let mirrored_addr = self.ppu.mirror_vram_addr(addr, mirroring);
                    // PPUの内部VRAMに書き込み
                    if mirrored_addr < self.ppu.vram.len() as u16 {
                        self.ppu.vram[mirrored_addr as usize] = data;
                    }
                }
            },
            0x3F00..=0x3FFF => {
                // パレットRAMへの書き込み
                self.write_palette(addr, data);
            },
            _ => {} // 不明なアドレス空間
        }
    }

    // --- System Clocking --- (修正版)
    // Step the system by one CPU instruction
    pub fn clock(&mut self) {
        // 1. CPUを一時的に取り出して命令を実行
        let mut cpu = std::mem::take(&mut self.cpu);
        let cpu_cycles = cpu.step(self);
        self.cpu = cpu; // CPUを戻す
        
        // アドレス0xD2の内容をモニター（バグ検出用）- 頻度増加
        if self.total_cycles % 50 == 0 {
            self.monitor_zero_page_d2();
        }
        
        // 2. PPUを一時的に取り出してサイクルを処理
        let mut ppu = std::mem::take(&mut self.ppu);
        let ppu_cycles = cpu_cycles as u64 * 3;
        
        // PPUのステップを進める（CPU:PPU = 1:3）
        for _ in 0..ppu_cycles {
            ppu.step(1, self);
        }
        
        // NMIフラグをチェック
        let nmi_triggered = ppu.nmi_triggered();
        let vblank_active = (ppu.status & 0x80) != 0; // VBlankフラグのチェック
        
        // VBlank発生時にゼロページ0xD2に値を設定（臨時対応）
        if vblank_active {
            self.cpu_ram.write(0xD2, 0x04); // アキュムレータと同じ値に変更（0x03→0x04）
            if self.total_cycles % 500 == 0 { // ここも頻度増加
                println!("Bus: VBlank中、アドレス0xD2に値4を設定しました");
                self.monitor_zero_page_d2();
            }
        }
        
        if nmi_triggered {
            ppu.clear_nmi_flag();
            println!("Bus: NMI検出、CPUにフラグを通知します");
        }
        
        // PPUを戻す
        self.ppu = ppu;
        
        // 3. NMI処理（必要な場合）
        if nmi_triggered {
            let mut cpu = std::mem::take(&mut self.cpu);
            cpu.nmi(self);
            self.cpu = cpu;
        }
        
        // 4. 合計サイクル数を更新
        self.total_cycles += cpu_cycles as u64;
    }

     // --- Getters for inspection/frontend ---
    pub fn get_ppu_frame(&self) -> FrameData {
         self.ppu.get_frame()
     }

     pub fn get_cpu_state(&self) -> InspectState {
        self.cpu.inspect()
     }

    // デバッグ用メモリ読み取りメソッドを追加（サイドエフェクトなし）
    pub fn debug_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                self.cpu_ram.read(addr & 0x07FF)
            }
            0x2000..=0x3FFF => {
                // PPUレジスタの現在の値を返す（状態変更なし）
                let mirrored_addr = 0x2000 + (addr & 0x0007);
                match mirrored_addr {
                    0x2002 => self.ppu.status, // STATUSレジスタをクリアしない
                    0x2004 => self.ppu.oam_data[self.ppu.oam_addr as usize],
                    0x2007 => {
                        if (self.ppu.vram_addr.get() >= 0x3F00) {
                            self.read_palette(self.ppu.vram_addr.get())
                        } else {
                            self.ppu.data_buffer
                        }
                    }
                    _ => 0
                }
            }
            0x4016 => 0, // コントローラー状態（読み取りのみ）
            0x4017 => 0,
            0x4000..=0x4015 | 0x4018..=0x401F => 0,
            0x4020..=0xFFFF => {
                match &self.cartridge {
                    Some(cart) => cart.read_prg(addr),
                    None => 0,
                }
            }
        }
    }

    // CPUが読んでいるゼロページアドレス0xD2をモニターするためのヘルパー関数
    pub fn monitor_zero_page_d2(&self) {
        let value = self.cpu_ram.read(0xD2);
        println!("ゼロページアドレス $D2 = {:02X} ({})", value, value);
    }

    // --- OAM DMA helper --- 
    fn trigger_oam_dma(&mut self, page: u8) {
        let start_addr = (page as u16) << 8;
        let mut oam_data_buffer = [0u8; 256];
        for i in 0..256 {
            // Fix the type mismatches (convert i from usize to u16)
            oam_data_buffer[i] = self.read(start_addr + (i as u16)); 
        }
        let start_idx = self.ppu.oam_addr as usize; 
        for i in 0..256 {
            let target_idx = (start_idx + i) % 256; 
            self.ppu.oam_data[target_idx] = oam_data_buffer[i];
        }
        // TODO: Stall the CPU 
        println!("[Bus] OAM DMA Triggered from page {:02X}", page);
    }

    // 特定のメモリ範囲の内容をダンプする機能
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
                    if value >= 32 && value <= 126 { // ASCII表示可能範囲
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

    // CPU命令のディスアセンブル機能（簡易版）
    pub fn debug_disassemble(&self, start_addr: u16, num_instructions: u16) {
        let mut addr = start_addr;
        
        println!("ディスアセンブル - アドレス ${:04X}から{}命令:", start_addr, num_instructions);
        
        for _ in 0..num_instructions {
            let opcode = self.debug_read(addr);
            
            // 命令長を決定（簡易版）
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
                    // 静的な文字列に変更
                    let op_str = match opcode {
                        0x00 => "BRK",
                        0xEA => "NOP",
                        // 他の特殊なオペコード
                        _ => "???",
                    };
                    (op_str, "")
                },
            };
            
            // アドレスモードに基づいてバイト数を決定
            let bytes = match addr_mode {
                "impl" => 1,
                "imm" | "zp" | "zp,X" | "zp,Y" | "rel" | "(ind,X)" | "(ind),Y" => 2,
                "abs" | "abs,X" | "abs,Y" | "(ind)" => 3,
                _ => 1,
            };
            
            // オペランドを取得
            let operand1 = if bytes > 1 { self.debug_read(addr + 1) } else { 0 };
            let operand2 = if bytes > 2 { self.debug_read(addr + 2) } else { 0 };
            
            // オペコードとオペランドをフォーマット
            let mut instr = format!("${:04X}: {:02X} ", addr, opcode);
            if bytes > 1 { instr.push_str(&format!("{:02X} ", operand1)); } else { instr.push_str("   "); }
            if bytes > 2 { instr.push_str(&format!("{:02X} ", operand2)); } else { instr.push_str("   "); }
            
            // ニーモニックとアドレスモードをフォーマット
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
}

// Default implementation for Bus
impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

// Need InspectState available in this scope
use crate::cpu::InspectState;
use crate::ppu::FrameData;
