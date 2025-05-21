use crate::ram::Memory;
use crate::cartridge::{Cartridge};
use crate::ppu::Ppu;
use crate::cpu::{self, Cpu6502};
use crate::controller::Controller;
use std::sync::{Arc, Mutex};
use std::cell::{RefCell, RefMut, UnsafeCell};
use crate::cpu::InspectState;
use crate::ppu::FrameData;
use crate::Mirroring;

// The main system bus, connecting CPU, PPU, RAM, Cartridge, etc.
pub struct Bus {
    pub cpu_ram: RefCell<Memory>,
    pub ppu: RefCell<Ppu>,
    pub cpu: RefCell<Cpu6502>,
    cartridge: Option<Arc<Mutex<Cartridge>>>,
    pub controller1: RefCell<Controller>,
    pub controller2: RefCell<Controller>,
    pub total_cycles: u64,
    pub prev_nmi_line: bool,
    pub test_mode: bool,
    pub test_pattern_rendered: bool,
    oam_dma_cycles_remaining: usize,
    oam_dma_page: u8,
    oam_dma_offset: u8,
    oam_dma_data: u8,
    irq_cooldown: UnsafeCell<u32>, // Use UnsafeCell for interior mutability
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            cpu_ram: RefCell::new(Memory::new()),
            ppu: RefCell::new(Ppu::new()),
            cpu: RefCell::new(Cpu6502::new()),
            cartridge: None,
            controller1: RefCell::new(Controller::new()),
            controller2: RefCell::new(Controller::new()),
            total_cycles: 0,
            prev_nmi_line: true,
            test_mode: false,
            test_pattern_rendered: false,
            oam_dma_cycles_remaining: 0,
            oam_dma_page: 0,
            oam_dma_offset: 0,
            oam_dma_data: 0,
            irq_cooldown: UnsafeCell::new(0),
        }
    }

    // Method to insert a cartridge into the bus
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(Arc::new(Mutex::new(cartridge)));
        self.reset(); // Reset system on cartridge insertion
    }

    fn bus_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram.borrow().ram[addr as usize & 0x07FF],
            0x2000..=0x3FFF => { // PPU Registers
                let register = addr & 0x0007;
                match register {
                    0x0002 => { // PPU Status Register ($2002)
                        // Only peek, side effects handled by caller (CPU)
                        self.ppu.borrow().read_status_peek()
                    }
                    0x0004 => self.ppu.borrow().read_oam_data(),
                    0x0007 => {
                        // Only peek data, side effects handled by caller (CPU)
                        let ppu = self.ppu.borrow();
                        let vram_addr = ppu.get_vram_address();
                        // Use ppu.read_data_peek which needs BusAccess itself for VRAM/CHR
                        // Pass self (which implements BusAccess) to it.
                        ppu.read_data_peek(self, vram_addr)
                    }
                    _ => 0,
                }
            }
            0x4000..=0x4015 => 0,
            0x4016 => self.controller1.borrow_mut().read(),
            0x4017 => self.controller2.borrow_mut().read(),
            0x4018..=0x401F => 0,
            0x4020..=0xFFFF => { // Cartridge
                // Limit IRQ vector read logging to reduce spam
                if addr == 0xFFFE || addr == 0xFFFF {
                    // Safe access to the UnsafeCell
                    let cooldown = unsafe { *self.irq_cooldown.get() };
                    
                    if cooldown == 0 {
                        // Only log IRQ vector reads occasionally
                        let value = self.cartridge.as_ref().map_or(0xFF, |cart| cart.lock().unwrap().read_prg(addr));
                        println!("IRQ vector read at ${:04X}: ${:02X} (ROM addr: ${:04X})", 
                            addr, value, addr & 0x7FFF);
                        
                        // Set cooldown safely with UnsafeCell
                        unsafe { *self.irq_cooldown.get() = 1000; }
                        
                        return value;
                    } else {
                        // Decrement cooldown counter safely
                        unsafe { *self.irq_cooldown.get() = cooldown.saturating_sub(1); }
                    }
                }
                
                // Regular cartridge read
                self.cartridge.as_ref().map_or(0xFF, |cart| cart.lock().unwrap().read_prg(addr))
            }
        }
    }

    // fn bus_write(&self, addr: u16, data: u8) {
    fn bus_write(&mut self, addr: u16, data: u8) { // Change &self to &mut self
        match addr {
            0x0000..=0x1FFF => self.cpu_ram.borrow_mut().write(addr & 0x07FF, data),
            0x2000..=0x3FFF => { // PPU Registers
                let register = addr & 0x0007;

                // --- Check for writes during rendering --- 
                let ppu_ref = self.ppu.borrow();
                let mask = ppu_ref.mask;
                let scanline = ppu_ref.scanline;
                if (mask.show_background() || mask.show_sprites()) && (scanline >= 0 && scanline <= 239) {
                    // Limit log spam
                    static mut RENDER_WRITE_WARN_COUNT: u32 = 0;
                    unsafe {
                        if RENDER_WRITE_WARN_COUNT < 50 { // Log first 50 warnings
                            println!(
                                "[WARN] PPU Write during render! Addr=${:04X} Data=${:02X} Scanline={}, Cycle={}", 
                                addr, data, scanline, ppu_ref.cycle
                            );
                            RENDER_WRITE_WARN_COUNT += 1;
                        } else if RENDER_WRITE_WARN_COUNT == 50 {
                            println!("[WARN] PPU Write during render: Further warnings suppressed...");
                            RENDER_WRITE_WARN_COUNT += 1;
                        }
                    }
                }
                drop(ppu_ref); // Explicitly drop the borrow
                // --- End Check --- 

                // Log PPU register writes // <<< Temporarily disable logging
                // println!("[PPU Write] Addr=${:04X} (Register ${:04X}) Data=${:02X}", addr, register, data);
                match register {
                    0x0000 => { // PPUCTRL ($2000)
                        println!("[PPU Write] PPUCTRL (${:04X}) write: ${:02X}", addr, data); // Log PPUCTRL writes
                        self.ppu.borrow_mut().write_ctrl(data)
                    },
                    0x0001 => self.ppu.borrow_mut().write_mask(data),
                    0x0003 => self.ppu.borrow_mut().write_oam_addr(data),
                    0x0004 => {
                        println!("[PPU Write] OAMDATA (${:04X}) write: ${:02X}", addr, data); // Log OAMDATA
                        self.ppu.borrow_mut().write_oam_data(data)
                    },
                    0x0005 => {
                        println!("[PPU Write] PPUSCROLL (${:04X}) write: ${:02X}", addr, data); // Log PPUSCROLL
                        self.ppu.borrow_mut().write_scroll(data)
                    },
                    0x0006 => {
                        println!("[PPU Write] PPUADDR (${:04X}) write: ${:02X}", addr, data); // Log PPUADDR
                        self.ppu.borrow_mut().write_addr(data)
                    },
                    0x0007 => {
                        println!("[PPU Write] PPUDATA (${:04X}) write: ${:02X}", addr, data); // Log PPUDATA
                        // Get VRAM address *before* potential write borrows
                        let vram_addr = self.ppu.borrow().vram_addr.get();
                        println!("  -> Target VRAM Addr = ${:04X}", vram_addr);

                        // Perform the actual write to VRAM/Palette/CHR
                        if vram_addr >= 0x3F00 {
                            println!("  -> Writing to Palette...");
                            self.write_palette(vram_addr, data); // Use internal palette helper
                        } else {
                            println!("  -> Writing to VRAM/CHR via BusAccess::ppu_write_vram...");
                            // Use the BusAccess trait method directly on self
                            self.ppu_write_vram(vram_addr, data);
                        }

                        // Increment PPU address *after* the write is done
                        // This separates the borrows and resolves E0502
                        self.ppu.borrow_mut().increment_vram_addr();
                        println!("  -> PPU VRAM address incremented.");
                    }
                    _ => {}
                }
            }
            0x4000..=0x4013 => {},
            0x4014 => {
                // println!("Write to $4014 (OAM DMA Trigger): ${:02X}", data);
                self.trigger_oam_dma(data);
            },
            0x4015 => {},
            0x4016 => self.controller1.borrow_mut().write(data),
            0x4017 => self.controller2.borrow_mut().write(data),
            0x4018..=0x401F => {},
            0x4020..=0xFFFF => { // Cartridge
                if let Some(cart) = &self.cartridge {
                    if let Ok(mut cart_guard) = cart.lock() {
                        if addr >= 0x8000 {
                            // Attempting to write to Cartridge space (usually ROM)
                            // Mappers like MMC1 might use this for configuration
                            // If the mapper specific cpu_write didn't handle it, it might be an error
                            // or for mappers like NROM (Mapper 0), it's disallowed.
                            if cart_guard.get_mapper_id() == 0 {
                                // warn!("Attempted write to PRG ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
                            } else {
                                // Handle writes for other mappers if necessary, though ideally
                                // the mapper's own cpu_write should handle configuration registers.
                            }
                        }
                        cart_guard.write_prg(addr, data);
                    }
                }
            }
        }
    }

    // Method to handle side effects of reading PPU status register $2002
    // This should be called by the CPU after reading $2002
    pub fn ppu_status_read_side_effects(&mut self) {
        self.ppu.borrow_mut().handle_status_read_side_effects();
    }

    // --- Palette RAM Access Helpers ---
    pub fn read_palette(&self, addr: u16) -> u8 {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // Mirror $3F1x to $3F0x
            _ => index,
        };
        self.ppu.borrow().palette_ram[mirrored_index]
    }

    pub fn write_palette(&mut self, addr: u16, data: u8) {
        println!("[write_palette] Addr=${:04X}, Data=${:02X}", addr, data); // ★★★ Log entry
        let mirrored_addr = addr & 0x3F1F; // Apply palette mirroring
        println!("[write_palette] Mirrored Addr = ${:04X}", mirrored_addr); // ★★★ Log mirrored
        let final_addr = match mirrored_addr {
            0x3F10 | 0x3F14 | 0x3F18 | 0x3F1C => {
                println!("[write_palette] Mirroring ${:04X} to ${:04X}", mirrored_addr, mirrored_addr - 0x10); // ★★★ Log specific mirroring
                mirrored_addr - 0x10
            },
            _ => mirrored_addr,
        };
        let palette_index = (final_addr & 0x1F) as usize; // Calculate index
        println!("[write_palette] Final Addr = ${:04X}, Index = {}", final_addr, palette_index); // ★★★ Log final addr and index
        // Write to PPU's internal palette RAM
        if palette_index < self.ppu.borrow().palette_ram.len() { // ★★★ Add bounds check ★★★
            self.ppu.borrow_mut().palette_ram[palette_index] = data;
            println!("[write_palette] Wrote to palette index {}", palette_index); // ★★★ Log success
        } else {
            println!("[write_palette] ERROR: Palette index {} out of bounds (size {})!", palette_index, self.ppu.borrow().palette_ram.len()); // ★★★ Log error
            // Optionally panic here if this should never happen
            // panic!("Palette index out of bounds!");
        }
    }

    // --- PPU VRAM Access Helpers ---
    pub fn ppu_read(&self, addr: u16) -> u8 {
        if addr >= 0x3F00 {
            self.read_palette(addr)
        } else {
            self.ppu_read_vram(addr)
        }
    }
    
    pub fn ppu_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if let Some(cart) = &self.cartridge {
                    if let Ok(mut cart_lock) = cart.lock() {
                        cart_lock.write_chr(addr, data);
                    }
                }
            },
            0x2000..=0x3EFF => self.ppu_write_vram(addr, data),
            0x3F00..=0x3FFF => self.write_palette(addr, data), // Write to Palette RAM
            _ => {} // 無効なアドレスは無視
        }
    }

    pub fn ppu_read_vram(&self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF;
        // println!("[PPU VRAM Read] Addr=${:04X}", addr); // Log PPU VRAM reads

        if addr <= 0x1FFF {
            // Reading from Pattern Table space ($0000-$1FFF)
            // Delegate to cartridge
            self.cartridge.as_ref().map_or(0, |cart| cart.lock().unwrap().read_chr(addr))
        } else if addr <= 0x3EFF {
            // Reading from Nametable space ($2000-$3EFF)
            let mirrored_addr = self.ppu.borrow().mirror_vram_addr(addr, self.get_mirroring());
            if mirrored_addr < self.ppu.borrow().vram.len() { // Check bounds
                let data = self.ppu.borrow().vram[mirrored_addr];
                // ★★★ Log Nametable Read ★★★
                println!("--- Nametable Read: OrigAddr:{:04X} Mirrored:{:04X} -> Data:{:02X} ---", addr, mirrored_addr, data);
                // ★★★ ここまで ★★★
                data
            } else {
                // eprintln!("Error: Mirrored VRAM address {:04X} (index {}) out of bounds for internal VRAM read (size {})",
                //         mirrored_addr, mirrored_addr, self.ppu.borrow().vram.len());
                0
            }
        } else {
            // Reading from Palette space ($3F00-$3FFF)
            // Handled by read_palette, this branch shouldn't be hit if ppu_read is used correctly.
            // eprintln!("Warning: ppu_read_vram called for palette address {:04X}", addr);
            self.read_palette(addr) // Fallback to read_palette
        }
    }

    pub fn ppu_write_vram(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x3FFF;
        println!("[ppu_write_vram] Addr=${:04X}, Data=${:02X}", addr, data); // ★★★ Log entry
        match addr {
            0x0000..=0x1FFF => { // Pattern Tables
                println!("[ppu_write_vram] Writing to Pattern Table (CHR)..."); // ★★★ Log path
                if let Some(cart) = &self.cartridge {
                    // Consider adding a check here if CHR is RAM or ROM
                    // For now, assume write is possible (might panic if ROM)
                     println!("[ppu_write_vram] Attempting cart.write_chr..."); // ★★★ Log before cart write
                    cart.lock().unwrap().write_chr(addr, data);
                     println!("[ppu_write_vram] cart.write_chr completed."); // ★★★ Log after cart write
                } else {
                    println!("[ppu_write_vram] No cartridge found for CHR write."); // ★★★ Log no cart
                }
                println!("[ppu_write_vram] Wrote to Pattern Table (CHR)."); // ★★★ Log path end
            }
            0x2000..=0x3EFF => { // Name Tables
                println!("[ppu_write_vram] Writing to Name Table..."); // ★★★ Log path
                let mirroring = self.get_mirroring(); // Use the unified method
                let mirrored_addr = self.ppu.borrow().mirror_vram_addr(addr, mirroring);
                 println!("[ppu_write_vram] Mirrored NT Addr = ${:04X}", mirrored_addr); // ★★★ Log mirrored addr
                 if (mirrored_addr as usize) < self.ppu.borrow().vram.len() { // ★★★ Add bounds check
                    self.ppu.borrow_mut().vram[mirrored_addr as usize] = data;
                    println!("[ppu_write_vram] Wrote to Name Table index {}.", mirrored_addr); // ★★★ Fix: Add argument
                 } else {
                     println!("[ppu_write_vram] ERROR: VRAM index {} out of bounds (size {})!", mirrored_addr, self.ppu.borrow().vram.len()); // ★★★ Log error
                     // panic!("[ppu_write_vram] VRAM index out of bounds!");
                 }
            }
            0x3F00..=0x3FFF => { // Palette RAM
                println!("[ppu_write_vram] Writing to Palette via ppu_write_vram..."); // ★★★ Log path
                self.write_palette(addr, data); // Forward to write_palette
                println!("[ppu_write_vram] Wrote to Palette via ppu_write_vram."); // ★★★ Log path end
            },
            _ => {println!("[ppu_write_vram] Invalid address range: ${:04X}", addr);} // ★★★ Log invalid range
        }
    }

    // ★★★ PPUを1サイクル進めるメソッドを追加 ★★★
    pub fn step_ppu(&self) {
        // PPUを1サイクル進める
        // step_cycle は bus: &impl BusAccess を要求するので self を渡す
        self.ppu.borrow_mut().step_cycle(self);
    }

    // --- System Clocking (Simplified) ---
    pub fn clock(&mut self) -> u64 {
        let mut cycles_executed = 0; // Initialize cycles executed

        // --- OAM DMA Processing ---
        if self.oam_dma_cycles_remaining > 0 {
            self.oam_dma_cycles_remaining -= 1;
            if self.oam_dma_cycles_remaining % 2 == 1 { // Read cycle
                let addr = ((self.oam_dma_page as u16) << 8) + self.oam_dma_offset as u16;
                self.oam_dma_data = self.bus_read(addr);
            } else { // Write cycle
                // Write to OAM data via PPU's method
                self.ppu.borrow_mut().write_oam_byte(self.oam_dma_offset, self.oam_dma_data);
                self.oam_dma_offset = self.oam_dma_offset.wrapping_add(1);
                if self.oam_dma_offset == 0 { // Finished writing 256 bytes
                    self.oam_dma_cycles_remaining = 0; // End DMA
                    // Potentially add 1 or 2 extra cycles if needed for odd/even alignment
                }
            }
            // _cycles = 1; // Don't increment CPU cycles during DMA
            // CPU does not execute during OAM DMA
            cycles_executed = 0; // Explicitly set to 0
        } else {
            // --- Normal CPU Clocking ---
            let bus_ptr = self as *mut Self; // Get raw pointer to self
            cycles_executed = {
                let mut cpu_ref = self.cpu.borrow_mut();
                // Use unsafe to pass mutable bus access to CPU clock
                // Ensure Cpu6502::step signature matches this call.
                unsafe { cpu_ref.step(&mut *bus_ptr) as u64 } // Cast result to u64
            };
        }

        // --- PPU Clocking ---
        self.clock_ppu(cycles_executed);

        // --- NMI Check (after PPU clocking) ---
        let current_nmi_line = self.ppu.borrow().nmi_line_low;
        if !current_nmi_line && self.prev_nmi_line { // Falling edge (true -> false)
            if self.ppu.borrow().ctrl.generate_nmi() {
                self.cpu.borrow_mut().trigger_nmi(); // Use trigger_nmi() method
                // println!("NMI triggered! Scanline: {}, Cycle: {}", self.ppu.borrow().scanline, self.ppu.borrow().cycle);
            }
        }
        self.prev_nmi_line = current_nmi_line;

        // Update total cycles
        self.total_cycles += cycles_executed; // Now both are u64

        cycles_executed // Return the number of CPU cycles executed (u64)
    }

    // Clock PPU based on CPU cycles executed
    fn clock_ppu(&mut self, cpu_cycles: u64) {
        let bus_ptr = self as *mut Self; // Get raw pointer to self for BusAccess
        for _ in 0..cpu_cycles * 3 {
            // Pass BusAccess via unsafe pointer to ppu.step_cycle
            let mut ppu = self.ppu.borrow_mut();
            unsafe { ppu.step_cycle(&mut *bus_ptr); }
        }
    }

    // --- Other Bus Methods ---
    pub fn get_ppu_frame(&self) -> FrameData {
        // TODO: Implement get_frame_data in ppu.rs or similar
        self.ppu.borrow().frame.clone()
    }

    pub fn get_ppu_frame_direct(&self) -> FrameData {
        self.ppu.borrow().frame.clone()
    }

    pub fn set_ppu_frame(&mut self, frame: FrameData) {
        self.ppu.borrow_mut().frame = frame;
    }

    pub fn get_cpu_state(&self) -> InspectState {
        self.cpu.borrow().inspect()
    }

    pub fn get_cpu_state_mut(&self) -> RefMut<'_, Cpu6502> {
        self.cpu.borrow_mut()
    }

    pub fn write_ppu_mask(&mut self, value: u8) {
        self.ppu.borrow_mut().write_mask(value);
    }

    pub fn is_frame_complete(&self) -> bool {
        self.ppu.borrow().frame_complete
    }

    pub fn reset_frame_complete(&mut self) {
        self.ppu.borrow_mut().frame_complete = false;
    }

    pub fn debug_read(&self, addr: u16) -> u8 {
         match addr {
            0x0000..=0x1FFF => self.cpu_ram.borrow().ram[addr as usize & 0x07FF],
            0x2000..=0x3FFF => { // PPU Registers
                let register = addr & 0x0007;
                match register {
                    0x0002 /* PPUSTATUS */ => 0, // TODO: Implement self.ppu.borrow().peek_status(),
                    0x0004 /* OAMDATA */ => self.ppu.borrow().read_oam_data(),
                    0x0007 /* PPUDATA */ => 0, // TODO: Implement self.ppu.borrow().peek_data_read_buffer(),
                    _ => 0,
                }
            }
            0x4016 => 0, // TODO: Implement self.controller1.borrow().peek(),
            0x4020..=0xFFFF => { // Cartridge
                self.cartridge.as_ref().map_or(0xFF, |cart| cart.lock().unwrap().read_prg(addr))
            }
            _ => 0,
        }
    }

    pub fn trigger_oam_dma(&mut self, page: u8) {
        if self.oam_dma_cycles_remaining > 0 {}
        self.oam_dma_page = page;
        self.oam_dma_offset = 0;
        self.oam_dma_cycles_remaining = 513;
        if self.total_cycles % 2 == 1 {
            self.oam_dma_cycles_remaining += 1;
        }
    }

    pub fn debug_memory_dump(&self, start_addr: u16, length: u16) {
        self.cpu.borrow().dump_memory(self, start_addr, length);
    }

    pub fn reset(&mut self) {
        self.cpu_ram = RefCell::new(Memory::new());
        // self.ppu = RefCell::new(Ppu::new()); // Don't create new PPU, reset existing one
        self.ppu.borrow_mut().reset(); // ★★★ Reset existing PPU instance ★★★
        // TODO: Implement reset in controller.rs
        // self.controller1.borrow_mut().reset();
        // self.controller2.borrow_mut().reset();
        self.total_cycles = 0;
        self.oam_dma_cycles_remaining = 0;
        
        // ROM読み込み確認
        if let Some(_) = &self.cartridge {
            // OK
        } else {
            println!("WARNING: Attempting to reset without cartridge loaded");
        }
        
        // CPU reset
        println!("[Bus Reset] Calling cpu.reset()..."); // Log before CPU reset
        let bus_ptr = self as *mut Self;
        let mut cpu_ref = self.cpu.borrow_mut();
        unsafe { cpu_ref.reset(&mut *bus_ptr); }
        println!("[Bus Reset] cpu.reset() finished."); // Log after CPU reset
    }

    pub fn toggle_test_mode(&mut self) {
        self.test_mode = !self.test_mode;
        println!("Test mode toggled: {}", self.test_mode);
        if self.test_mode {
            self.test_pattern_rendered = false;
        } else {
            println!("Exiting test mode.");
            self.reset();
        }
    }

    pub fn handle_key_event(&mut self, key_code: &str, pressed: bool) {
        // TODO: Implement handle_key in controller.rs
        // self.controller1.borrow_mut().handle_key(key_code, pressed);
        println!("Ignoring key event for now: {} ({})", key_code, pressed);
    }

    pub fn set_cpu_pc(&mut self, addr: u16) {
        self.cpu.borrow_mut().registers.program_counter = addr;
        println!("Debug: Set CPU PC to ${:04X}", addr);
    }

    pub fn get_ppu(&self) -> RefMut<'_, Ppu> {
        self.ppu.borrow_mut()
    }

    pub fn get_cpu(&self) -> RefMut<'_, Cpu6502> {
        self.cpu.borrow_mut()
    }

    pub fn init_test_pattern(&mut self) {
        println!("Initializing PPU VRAM with test pattern...");
        let mut ppu = self.ppu.borrow_mut();
        for i in 0..960 { // Fill Name Table 0
            let tile_index = (i % 32) as u8;
            ppu.vram[i] = tile_index;
        }
        for i in 0..64 { // Fill Attribute Table 0
            let row_group = i / 8;
            let color_bits = match row_group % 4 {
                0 => 0b00000000, 1 => 0b01010101, 2 => 0b10101010, _ => 0b11111111,
            };
            ppu.vram[0x3C0 + i] = color_bits;
        }
        let colors: [u8; 12] = [0x11, 0x21, 0x31, 0x0F, 0x1F, 0x2F, 0x06, 0x16, 0x26, 0x09, 0x19, 0x29];
        for i in 0..12 {
             ppu.write_palette(0x01 + i as u8, colors[i]);
        }
        ppu.write_palette(0x00, 0x0D);
        println!("Test pattern VRAM initialization complete.");
        self.test_pattern_rendered = true;
    }

    pub fn is_brk_detected(&self) -> bool {
        self.cpu.borrow().is_brk_executed()
    }

    pub fn get_ppu_test_frame(&mut self) -> Result<FrameData, String> {
        if !self.test_pattern_rendered {
            self.init_test_pattern();
        }

        let mut ppu = self.ppu.borrow_mut();
        // ★★★ Initialize FrameData with width and height ★★★
        let mut frame = FrameData::new(256, 240);

        for y in 0..240 {
            for x in 0..256 {
                let nt_col = x / 8;
                let nt_row = y / 8;
                let nt_index = nt_row * 32 + nt_col;
                let tile_id = ppu.vram[nt_index as usize];

                let pattern_addr = (tile_id as u16 * 16) + (y % 8) as u16;
                let pattern_low = self.ppu_read_vram(pattern_addr);
                let pattern_high = self.ppu_read_vram(pattern_addr + 8);

                let bit_index = 7 - (x % 8);
                let color_bit_low = (pattern_low >> bit_index) & 1;
                let color_bit_high = (pattern_high >> bit_index) & 1;
                let palette_entry_index = (color_bit_high << 1) | color_bit_low;

                let attr_col = nt_col / 4;
                let attr_row = nt_row / 4;
                let attr_byte_index = 0x3C0 + attr_row * 8 + attr_col;
                let attr_byte = ppu.vram[attr_byte_index as usize];
                let quadrant_shift = ((nt_col % 4) / 2) * 2 + ((nt_row % 4) / 2) * 4;
                let palette_high_bits = (attr_byte >> quadrant_shift) & 0b11;

                let palette_addr = if palette_entry_index == 0 {
                    0x3F00
                } else {
                    0x3F01 + ((palette_high_bits as u16) << 2) + (palette_entry_index as u16 - 1)
                };

                let color_index = self.read_palette(palette_addr);
                // ★★★ Use dummy RGB value for missing get_nes_color ★★★
                let (r, g, b) = (color_index, color_index, color_index);

                let pixel_index = y * 256 + x;
                // ★★★ Use frame.pixels and check bounds ★★★
                let base_pixel_idx = pixel_index * 3;
                if base_pixel_idx + 2 < frame.pixels.len() {
                    frame.pixels[base_pixel_idx] = r;
                    frame.pixels[base_pixel_idx + 1] = g;
                    frame.pixels[base_pixel_idx + 2] = b;
                } else {
                    // println!("Warning: Pixel index out of bounds: {}", pixel_index);
                }
            }
        }
        Ok(frame)
    }

    pub fn is_rom_loaded(&self) -> bool {
        self.cartridge.is_some()
    }

    // tickメソッドを追加 - clock()ラッパー
    pub fn tick(&mut self) -> Option<bool> {
        self.clock();
        Some(self.ppu.borrow().frame_complete)
    }

    // mem_readメソッドを追加 - readのエイリアス
    pub fn mem_read(&self, addr: u16) -> u8 {
        self.read(addr)
    }
}

// BusAccessトレイトを公開定義
pub trait BusAccess {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, data: u8);
    fn ppu_status_read_side_effects(&mut self); // Add method for $2002 read side effects
    fn ppu_data_read_side_effects(&mut self, last_read_value: u8) -> u8; // Add method for $2007 read side effects
    fn ppu_read_vram(&self, addr: u16) -> u8; // Add method for PPU VRAM/CHR reads
    fn ppu_write_vram(&mut self, addr: u16, data: u8); // Add method for PPU VRAM/CHR writes
    fn get_mirroring(&self) -> Mirroring; // <<< NEW: Method to get current mirroring mode
    fn read_u16_zp(&self, addr: u16) -> u16; // ゼロページラップアラウンド付き 16 ビット読み込み

    // 16ビット読み込み用ヘルパー（デフォルト実装）
    fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }
    
    // 16ビット書き込み用ヘルパー（デフォルト実装）
    fn write_u16(&mut self, addr: u16, data: u16) {
        let lo = (data & 0xFF) as u8;
        let hi = (data >> 8) as u8;
        self.write(addr, lo);
        self.write(addr.wrapping_add(1), hi);
    }
}

// Bus構造体にBusAccessを実装
impl BusAccess for Bus {
    fn read(&self, addr: u16) -> u8 {
        self.bus_read(addr)
    }
    
    fn write(&mut self, addr: u16, data: u8) {
        self.bus_write(addr, data);
    }

    fn ppu_status_read_side_effects(&mut self) {
        self.ppu.borrow_mut().handle_status_read_side_effects();
    }

    fn ppu_data_read_side_effects(&mut self, last_read_value: u8) -> u8 {
        self.ppu.borrow_mut().handle_data_read_side_effects(last_read_value)
    }

    fn ppu_read_vram(&self, addr: u16) -> u8 {
        self.ppu_read_vram(addr)
    }

    fn ppu_write_vram(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => {
                if let Some(cart) = &self.cartridge {
                    cart.lock().unwrap().write_chr(addr, data);
                }
            },
            0x2000..=0x3EFF => {
                let mirroring = self.get_mirroring();
                let mirrored_addr = self.ppu.borrow().mirror_vram_addr(addr, mirroring);
                if mirrored_addr < self.ppu.borrow().vram.len() {
                    self.ppu.borrow_mut().vram[mirrored_addr] = data;
                }
            },
            0x3F00..=0x3FFF => {
                self.write_palette(addr, data);
            },
            _ => {}
        }
    }

    fn get_mirroring(&self) -> Mirroring {
        self.cartridge.as_ref().map_or(Mirroring::Horizontal, |cart| cart.lock().unwrap().get_mirroring())
    }

    fn read_u16_zp(&self, addr: u16) -> u16 {
        let lo_addr = addr & 0x00FF;
        let hi_addr = (addr.wrapping_add(1)) & 0x00FF;
        let lo = self.read(lo_addr) as u16;
        let hi = self.read(hi_addr) as u16;
        (hi << 8) | lo
    }
}
