use serde::Serialize; // Import Serialize
use crate::Mirroring; // Ensure Mirroring is imported from crate root (main.rs)
use crate::bus::Bus;             // Ensure Bus is imported

// use crate::bus::Bus; // Removed unused import

// NES -> RGB color conversion lookup table
// (Using a common palette like Nestopia's NTSC)
const NES_PALETTE: [(u8, u8, u8); 64] = [
    (84, 84, 84), (0, 30, 116), (8, 16, 144), (48, 0, 136), (68, 0, 100), (92, 0, 48), (84, 4, 0), (60, 24, 0),
    (32, 42, 0), (8, 58, 0), (0, 64, 0), (0, 60, 0), (0, 50, 60), (0, 0, 0), (0, 0, 0), (0, 0, 0),
    (152, 150, 152), (8, 76, 196), (48, 50, 236), (92, 30, 228), (136, 20, 176), (160, 20, 100), (152, 34, 32), (120, 60, 0),
    (84, 90, 0), (40, 114, 0), (8, 124, 0), (0, 118, 40), (0, 102, 120), (0, 0, 0), (0, 0, 0), (0, 0, 0),
    (236, 238, 236), (76, 154, 236), (120, 124, 236), (176, 98, 236), (228, 84, 236), (236, 88, 180), (236, 106, 100), (212, 136, 32),
    (160, 170, 0), (116, 196, 0), (76, 208, 32), (56, 204, 108), (56, 180, 220), (60, 60, 60), (0, 0, 0), (0, 0, 0),
    (236, 238, 236), (168, 204, 236), (188, 188, 236), (212, 178, 236), (236, 174, 236), (236, 174, 212), (236, 180, 176), (228, 196, 144),
    (204, 210, 120), (180, 222, 120), (168, 226, 144), (152, 226, 180), (160, 214, 228), (160, 162, 160), (0, 0, 0), (0, 0, 0),
];

const SCREEN_WIDTH: usize = 256;
const SCREEN_HEIGHT: usize = 240;
const CYCLES_PER_SCANLINE: u64 = 341;
const SCANLINES_PER_FRAME: u64 = 262; // Includes VBlank scanlines
const STATUS_VBLANK: u8 = 0x80;
const CTRL_VRAM_INCREMENT: u8 = 0x04;

#[derive(Default, Debug, Clone, Copy, Serialize)]
pub struct VRamRegister {  // Make the struct public
   pub address: u16, // Make the field public (internal representation, 15 bits used)
}

impl VRamRegister {
    pub fn get(&self) -> u16 { self.address & 0x3FFF } // Make public - PPU addresses are 14-bit
    pub fn set(&mut self, addr: u16) { self.address = addr & 0x7FFF; } // Make public - Internal register is 15 bits
    pub fn increment(&mut self, amount: u16) { self.address = self.address.wrapping_add(amount); } // Make public
    pub fn coarse_x(&self) -> u8 { (self.address & 0x001F) as u8 } // Make public
    pub fn coarse_y(&self) -> u8 { ((self.address >> 5) & 0x001F) as u8 } // Make public
    pub fn nametable_select(&self) -> u8 { ((self.address >> 10) & 0x0003) as u8 } // Make public
    pub fn fine_y(&self) -> u8 { ((self.address >> 12) & 0x0007) as u8 } // Make public
    pub fn set_coarse_x(&mut self, coarse_x: u8) { self.address = (self.address & !0x001F) | (coarse_x as u16 & 0x1F); } // Make public
    pub fn set_coarse_y(&mut self, coarse_y: u8) { self.address = (self.address & !0x03E0) | ((coarse_y as u16 & 0x1F) << 5); } // Make public
    pub fn set_nametable_select(&mut self, nt: u8) { self.address = (self.address & !0x0C00) | ((nt as u16 & 0x03) << 10); } // Make public
    pub fn set_fine_y(&mut self, fine_y: u8) { self.address = (self.address & !0x7000) | ((fine_y as u16 & 0x07) << 12); } // Make public
    pub fn copy_horizontal_bits(&mut self, t: &VRamRegister) { self.address = (self.address & !0x041F) | (t.address & 0x041F); } // Make public
    pub fn copy_vertical_bits(&mut self, t: &VRamRegister) { self.address = (self.address & !0x7BE0) | (t.address & 0x7BE0); } // Make public
}

#[derive(Clone, Serialize)] // Add Serialize here
pub struct FrameData { // Made public
    pub pixels: Vec<u8>,  // Public for access from frontend if needed (or provide getter)
    pub width: usize,     // Use usize
    pub height: usize,    // Use usize
}

impl FrameData {
    pub fn new(width: usize, height: usize) -> Self { // Made public
        Self {
            pixels: vec![0; width * height * 4], // RGBA
            width,
            height,
        }
    }
}

pub struct Ppu { // Make Ppu public
    // PPUレジスタ (Busからアクセスするため pub に変更)
    pub ctrl: u8,       // $2000 Write
    pub mask: u8,       // $2001 Write
    pub status: u8,     // $2002 Read
    pub oam_addr: u8,   // $2003 Write
    // oam_data: u8, // $2004 Read/Write (Direct OAM access often handled differently)

    // Internal state (Busからアクセスするため pub に変更)
    pub cycle: u64,          // Cycle count for the current scanline
    pub scanline: u64,       // Current scanline number (0-261)
    pub frame: FrameData,    // Framebuffer for the current frame
    pub nmi_occurred: bool,  // Flag indicating NMI should be triggered
    pub nmi_output: bool,    // Whether NMI generation is enabled (from CTRL register)

    // VRAM / Address Registers (Busからアクセスするため pub に変更)
    pub vram_addr: VRamRegister, // Use the VRamRegister struct
    pub temp_vram_addr: VRamRegister, // Temporary VRAM address ('t')
    pub address_latch_low: bool, // For $2005/$2006 writes
    pub fine_x_scroll: u8, // Fine X scroll (3 bits)

    // Data I/O (Busからアクセスするため pub に変更)
    pub data_buffer: u8,   // Buffer for $2007 reads

    // OAM (Object Attribute Memory) (Busからアクセスするため pub に変更)
    pub oam_data: [u8; 256],

    // Palette RAM (32 bytes)
    pub palette_ram: [u8; 32],

    pub vram: [u8; 2048],         // Nametable RAM (2KB)
    pub frame_complete: bool,
}

impl Ppu {
    pub fn new() -> Self { // Make new public
        Self {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            // scroll: 0, // Removed field
            // addr: 0,   // Removed field

            cycle: 0,
            scanline: 0,
            frame: FrameData::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            nmi_occurred: false,
            nmi_output: false, // Initially disabled

            // Initialize new fields
            vram_addr: VRamRegister::default(),
            temp_vram_addr: VRamRegister::default(),
            address_latch_low: true, // First write to $2006 sets high byte
            fine_x_scroll: 0,
            data_buffer: 0,
            oam_data: [0; 256],
            palette_ram: [0; 32], // Initialize palette RAM
            vram: [0; 2048],
            frame_complete: false,
        }
    }

    // Simulate PPU stepping for a given number of PPU cycles (CPU cycles * 3)
    pub fn step(&mut self, ppu_cycles: u64, bus: &super::bus::Bus) {
        for _ in 0..ppu_cycles {
            // --- Cycle and Scanline Logic ---
            self.cycle = self.cycle.wrapping_add(1);
            if self.cycle >= CYCLES_PER_SCANLINE {
                self.cycle = 0; // Reset cycle for new scanline
                self.scanline = self.scanline.wrapping_add(1);

                // VBlank開始時の処理を強化
                if self.scanline == 241 { 
                    // Set VBlank flag in STATUS register
                    self.status |= STATUS_VBLANK; 
                    
                    // デバッグ出力を詳細化
                    println!("VBlank開始(scanline 241): STATUS:{:02X} → VBlankフラグを設定しました", self.status);
                    
                    // トリガーNMI（CTRL レジスタのbit 7がセットされている場合）
                    if (self.ctrl & 0x80) != 0 {
                        self.nmi_occurred = true;
                        // デバッグ出力
                        println!("NMI triggered at scanline 241! STATUS:{:02X}, NMI_OCCURRED:true", self.status);
                    }
                    
                    // さらに詳細なデバッグ情報
                    println!("VBlank詳細: CTRL:{:02X} MASK:{:02X} STATUS:{:02X} NMI_OCCURRED:{} NMI_OUTPUT:{}",
                        self.ctrl, self.mask, self.status, self.nmi_occurred, self.nmi_output);
                } 
                // フレーム終了時の処理
                else if self.scanline >= SCANLINES_PER_FRAME { 
                    self.scanline = 0; // Reset for new frame
                    self.frame_complete = true; // Set flag for frame completion
                    self.status &= !STATUS_VBLANK; // Clear VBlank flag
                    self.nmi_occurred = false; // Reset NMI occurred flag
                    
                    // デバッグ出力
                    println!("Frame complete! VBlank flag cleared.");
                }
            }

            // --- Debug Print BEFORE Rendering Check ---
            if self.scanline < 240 && self.cycle < 5 {
                println!(
                    "Scanline {:>3}, Cycle {:>3}: CTRL:{:02X} MASK:{:02X} STATUS:{:02X} VADDR:{:04X}",
                    self.scanline, self.cycle, self.ctrl, self.mask, self.status, self.vram_addr.get()
                );
            }
            
            // VBlank期間中のデバッグ出力（定期的に）
            if self.scanline >= 241 && self.scanline < 261 && self.cycle == 1 {
                println!(
                    "VBlank scanline {:>3}: STATUS:{:02X}, NMI_OUTPUT:{}, NMI_OCCURRED:{}",
                    self.scanline, self.status, self.nmi_output, self.nmi_occurred
                );
            }

            // --- PPU Rendering Logic (Simplified) ---
            let rendering_enabled = self.mask & 0x18 != 0; // Show background or sprites enabled
            let on_visible_scanline = self.scanline < 240;
            let on_visible_cycle = self.cycle >= 1 && self.cycle <= 256;
            let on_prefetch_cycle = self.cycle >= 321 && self.cycle <= 336; // Cycles for next scanline prefetch

            if rendering_enabled && on_visible_scanline && (on_visible_cycle || on_prefetch_cycle) {
                // Simplified fetching logic (real PPU uses shift registers and fetches ahead)
                let fine_y = (self.vram_addr.get() >> 12) & 0x0007;

                // --- 1. Fetch Nametable Byte ---
                let nt_addr = 0x2000 | (self.vram_addr.get() & 0x0FFF);
                let tile_id = bus.ppu_read_vram(nt_addr);

                // --- 2. Fetch Attribute Table Byte ---
                let at_addr = 0x23C0 | (self.vram_addr.get() & 0x0C00) | ((self.vram_addr.get() >> 4) & 0x38) | ((self.vram_addr.get() >> 2) & 0x07);
                let at_byte = bus.ppu_read_vram(at_addr);
                let at_shift = ((self.vram_addr.get() >> 4) & 0x04) | (self.vram_addr.get() & 0x02);
                let palette_high_bits = ((at_byte >> at_shift) & 0x03) << 2;

                // --- 3. Fetch Background Tile Pattern (Low Bit Plane) ---
                let pattern_table_base = if self.ctrl & 0x10 == 0 { 0x0000 } else { 0x1000 };
                let pattern_addr_low = pattern_table_base + (tile_id as u16 * 16) + fine_y;
                let pattern_low = bus.ppu_read_vram(pattern_addr_low);

                // --- 4. Fetch Background Tile Pattern (High Bit Plane) ---
                let pattern_addr_high = pattern_addr_low + 8;
                let pattern_high = bus.ppu_read_vram(pattern_addr_high);

                // --- Render Pixel (if in visible cycle range) ---
                if on_visible_cycle {
                    let x = (self.cycle - 1) as usize;
                    let y = self.scanline as usize;

                    let fine_x_shift = 7 - (self.fine_x_scroll & 0x07);
                    let pixel_low_bit = (pattern_low >> fine_x_shift) & 1;
                    let pixel_high_bit = (pattern_high >> fine_x_shift) & 1;
                    let palette_low_bits = (pixel_high_bit << 1) | pixel_low_bit;

                    let universal_bg_idx = bus.read_palette(0x3F00); // Universal background
                    let final_palette_index = if palette_low_bits == 0 {
                        universal_bg_idx
                    } else {
                        palette_high_bits | palette_low_bits // Combine AT bits and Pattern bits
                    };

                    // --- Modified DEBUG OUTPUT (inside rendering block) ---
                    // Print for a few pixels on specific scanlines
                    if x < 5 && (y == 10 || y == 20 || y == 30) {
                        println!(
                            "  Pixel({:>3},{:>3}): MASK:{:02X} T:{:02X} AT:{:02X} PL:{:02X} PH:{:02X} PB:{:X} PLB:{:X} -> Idx:{:02X} (Uni:{:02X})",
                            x, y, self.mask, tile_id, at_byte, pattern_low, pattern_high, palette_high_bits, palette_low_bits, final_palette_index, universal_bg_idx
                        );
                    }
                    // --- END DEBUG ---

                    let (r, g, b) = NES_PALETTE[final_palette_index as usize & 0x3F];

                    let frame_idx = (y * SCREEN_WIDTH + x) * 4;
                    if frame_idx + 3 < self.frame.pixels.len() {
                        self.frame.pixels[frame_idx] = r;
                        self.frame.pixels[frame_idx + 1] = g;
                        self.frame.pixels[frame_idx + 2] = b;
                        self.frame.pixels[frame_idx + 3] = 255; // Alpha
                    }
                }

                // Simplified VRAM address increment logic (Happens at specific cycles, not every fetch)
                 if self.cycle % 8 == 0 && on_visible_cycle { // Increment coarse X every 8 cycles within visible area
                    if (self.vram_addr.get() & 0x001F) == 31 {
                        self.vram_addr.address &= !0x001F;
                        self.vram_addr.address ^= 0x0400;
                    } else {
                        self.vram_addr.address = self.vram_addr.address.wrapping_add(1);
                    }
                }
                // Increment fine_x scroll - this happens regardless of the % 8 check
                 // Should really be tied to pixel generation/shift register clocking
                if on_visible_cycle || on_prefetch_cycle { // Need to advance fine_x for rendering and prefetch
                   self.fine_x_scroll = (self.fine_x_scroll + 1) & 0x07; // 巡回計算を修正（モジュロ演算の代わりにマスク）
                }

            }
             // Handle vertical increment at cycle 257 (or similar timing)
            if rendering_enabled && self.cycle == 257 && on_visible_scanline {
                // Increment vertical part of VRAM address ('v')
                if (self.vram_addr.get() & 0x7000) != 0x7000 { // if fine Y < 7
                    self.vram_addr.address = self.vram_addr.address.wrapping_add(0x1000); // Increment fine Y
                } else {
                    self.vram_addr.address &= !0x7000; // Fine Y = 0
                    let mut y = (self.vram_addr.get() & 0x03E0) >> 5; // let y = coarse Y
                    if y == 29 {
                        y = 0; // Coarse Y = 0
                        self.vram_addr.address ^= 0x0800; // switch vertical nametable
                    } else if y == 31 {
                        y = 0; // Coarse Y = 0, nametable not switched
                    } else {
                        y += 1; // Increment coarse Y
                    }
                    self.vram_addr.address = (self.vram_addr.address & !0x03E0) | (y << 5); // Put coarse Y back into v
                }
            }

            // --- Copy horizontal bits from t to v at cycle 257 --- (Simplified)
            if rendering_enabled && self.cycle == 257 {
                 // self.vram_addr.address = (self.vram_addr.address & 0xFBE0) | (self.temp_vram_addr.address & 0x041F);
                 // TODO: Implement temp_vram_addr logic for scrolling
            }
             // --- Copy vertical bits from t to v at end of VBlank --- (Simplified)
            if rendering_enabled && self.scanline == 261 && self.cycle == 1 { // Around the start of the visible frame
                // self.vram_addr.address = (self.vram_addr.address & 0x841F) | (self.temp_vram_addr.address & 0x7BE0);
                // TODO: Implement temp_vram_addr logic for scrolling
            }

        }
    }

    // Helper to determine VRAM address increment based on CTRL register bit 2
    // This can potentially be moved to Bus as well if PPU fields are public
    pub(super) fn get_vram_increment(&self) -> u16 { // Changed to pub(super) or keep private/move
        if (self.ctrl & 0x04) == 0 { 1 } else { 32 }
    }

    // Unified register access method for Bus to use
    // This avoids multiple &mut self borrows in bus.rs
    pub fn access_register(&mut self, addr: u16, data: u8, is_write: bool, bus: Option<&mut Bus>) -> u8 {
        let register = 0x2000 | (addr & 0x7); // Map $2000-$3FFF to $2000-$2007
        
        if is_write {
            // Write operation
            match register {
                0x2000 => self.write_ctrl(data),
                0x2001 => self.write_mask(data),
                0x2003 => self.write_oam_addr(data),
                0x2004 => self.write_oam_data(data),
                0x2005 => self.write_scroll(data),
                0x2006 => self.write_addr(data),
                0x2007 => if let Some(bus_ref) = bus { self.write_data(bus_ref, data) },
                _ => {} // $2002 is read-only
            }
            0 // Write operations return 0 by convention
        } else {
            // Read operation
            match register {
                0x2002 => self.read_status(),
                0x2004 => self.read_oam_data(),
                0x2007 => if let Some(bus_ref) = bus { self.read_data(bus_ref) } else { 0 },
                _ => 0 // Other registers return 0 when read (implementation-specific)
            }
        }
    }

    // --- PPU Register Read/Write Handlers (called by Bus) ---

    pub fn read_status(&mut self) -> u8 {
        let result = self.status | (self.data_buffer & 0x1F); // Combine status flags and data buffer bits
        self.status &= !STATUS_VBLANK; // Clear VBlank flag on read
        self.address_latch_low = true; // Reset address latch
        // Reading status might clear NMI flag (check timing/behavior)
        // self.nmi_occurred = false; 
        result
    }

    pub fn read_data(&mut self, bus: &Bus) -> u8 {
        let addr = self.vram_addr.get(); // Get current VRAM address (14-bit)
        let vram_increment = self.get_vram_increment();

        let result = if addr >= 0x3F00 { // Palette read
            let palette_data = bus.read_palette(addr);
            // Buffer update for palette reads uses mirrored VRAM address
            // Read from VRAM *before* the palette range ($2F00-$2FFF typically mirrors $3F00-$3FFF)
            self.data_buffer = bus.ppu_read_vram(addr.wrapping_sub(0x1000));
            palette_data // Palette data is returned directly
        } else { // VRAM read
            let vram_data = self.data_buffer; // Return buffered data
            self.data_buffer = bus.ppu_read_vram(addr); // Update buffer with current VRAM content
            vram_data
        };

        self.vram_addr.increment(vram_increment); // Increment VRAM address
        result
    }

    pub fn write_ctrl(&mut self, data: u8) {
        let old_nmi_output = self.nmi_output;
        self.ctrl = data;
        self.nmi_output = (data & 0x80) != 0; // Update NMI output flag
        // Trigger NMI if VBlank is set and NMI just became enabled
        if !old_nmi_output && self.nmi_output && (self.status & STATUS_VBLANK) != 0 {
            self.ppu_trigger_nmi(); // Call helper to trigger NMI
        }
        // Update temporary VRAM address nametable select bits ($2000 write part 1)
        self.temp_vram_addr.set_nametable_select(data & 0x03);
    }

    pub fn write_mask(&mut self, data: u8) {
        self.mask = data;
    }

    pub fn write_oam_addr(&mut self, data: u8) {
        self.oam_addr = data;
    }

    pub fn write_oam_data(&mut self, data: u8) {
        // TODO: Consider PPU rendering state (writes ignored during certain periods)
        self.oam_data[self.oam_addr as usize] = data;
        self.oam_addr = self.oam_addr.wrapping_add(1); // Increment OAM address
    }

    pub fn write_scroll(&mut self, data: u8) {
        if self.address_latch_low { // First write (X scroll)
            self.temp_vram_addr.set_coarse_x(data >> 3);
            self.fine_x_scroll = data & 0x07;
            self.address_latch_low = false; // Toggle latch
        } else { // Second write (Y scroll)
            self.temp_vram_addr.set_coarse_y(data >> 3);
            self.temp_vram_addr.set_fine_y(data & 0x07);
            self.address_latch_low = true; // Toggle latch
        }
    }

    pub fn write_addr(&mut self, data: u8) {
        if self.address_latch_low { // First write (High byte)
            // Clear upper byte and set bits 8-13 from data (masked to 6 bits)
            self.temp_vram_addr.address = (self.temp_vram_addr.address & 0x00FF) | (((data & 0x3F) as u16) << 8);
            self.address_latch_low = false; // Toggle latch
        } else { // Second write (Low byte)
            // Clear lower byte and set bits 0-7 from data
            self.temp_vram_addr.address = (self.temp_vram_addr.address & 0xFF00) | (data as u16);
            // Copy temporary address to VRAM address
            self.vram_addr = self.temp_vram_addr;
            self.address_latch_low = true; // Toggle latch
        }
    }

    pub fn write_data(&mut self, bus: &mut Bus, data: u8) {
        let addr = self.vram_addr.get(); // Get current VRAM address
        let vram_increment = self.get_vram_increment();

        if addr >= 0x3F00 { // Palette write
            bus.write_palette(addr, data);
        } else { // VRAM write
            bus.ppu_write_vram(addr, data);
        }

        self.vram_addr.increment(vram_increment); // Increment VRAM address
    }

    // --- NMI Helper ---
    fn ppu_trigger_nmi(&mut self) {
        // This logic might need refinement based on exact NMI timing requirements
        self.nmi_occurred = true;
        println!("PPU: NMI発生フラグをセット: nmi_occurred={}, status={:02X}, ctrl={:02X}", 
            self.nmi_occurred, self.status, self.ctrl);
    }

    // --- Helper for Nametable Mirroring (Called by Bus read/write helpers) ---
    pub fn mirror_vram_addr(&self, addr: u16, mode: Mirroring) -> u16 {
         let vram_index = addr & 0x0FFF; // Address within the 4KB VRAM space ($2000-$2FFF)
         let nametable_index = vram_index / 0x0400; // Which nametable (0, 1, 2, 3)

         match mode {
             Mirroring::Vertical => match nametable_index {
                 0 | 2 => vram_index & 0x03FF, // Map NT 0/2 to Physical NT 0 range (0x0000-0x03FF)
                 1 | 3 => (vram_index & 0x03FF) + 0x0400, // Map NT 1/3 to Physical NT 1 range (0x0400-0x07FF)
                 _ => unreachable!(),
             },
             Mirroring::Horizontal => match nametable_index {
                 0 | 1 => vram_index & 0x03FF, // Map NT 0/1 to Physical NT 0 range
                 2 | 3 => (vram_index & 0x03FF) + 0x0400, // Map NT 2/3 to Physical NT 1 range
                 _ => unreachable!(),
             },
              // Assuming SingleScreen maps to the first physical bank for simplicity
             Mirroring::SingleScreenLower => vram_index & 0x03FF,
             // Assuming SingleScreenUpper maps to the second physical bank (if HW supports it differently adjust)
             Mirroring::SingleScreenUpper => (vram_index & 0x03FF) + 0x0400,
             // FourScreen needs external RAM mapped appropriately by the cartridge/mapper.
             // This basic mirroring assumes 2KB internal VRAM.
             Mirroring::FourScreen => vram_index, // No mapping applied here, relies on mapper/bus
         }
     }

    // Remove duplicate/commented out render methods
    // pub fn render_scanline(&mut self) { ... }
    // pub fn render_frame(&mut self) { ... }
    // pub fn start_vblank(&mut self) { ... }
    // pub fn end_vblank(&mut self) { ... }
    // pub fn render_background(&mut self) { ... }
    // pub fn render_sprites(&mut self) { ... }

// ... rest of impl Ppu ...

    pub fn nmi_triggered(&self) -> bool {
        self.nmi_occurred
    }

    pub fn clear_nmi_flag(&mut self) {
        self.nmi_occurred = false;
    }

    // Returns a clone of the current frame data
    pub fn get_frame(&self) -> FrameData { // Added get_frame
        // TODO: Actually render pixels into self.frame based on PPU state
        // For now, return a clone of the (likely blank) frame buffer
        self.frame.clone()
    }

    // Helper for reading OAM data directly from Bus
    pub fn read_oam_data(&self) -> u8 {
        // Get current OAM address and read data at that address
        self.oam_data[self.oam_addr as usize]
        // Note: During rendering, reads should return 0xFF depending on timing,
        // but we'll implement that refinement later if needed
    }

}

// Default implementation for Ppu
impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
