    // src-tauri/src/registers.rs

    // --- PPU Control Register ($2000) ---
    #[derive(Debug, Default, Clone, Copy)]
    #[repr(transparent)]
    pub struct ControlRegister {
        bits: u8,
    }

    impl ControlRegister {
        pub fn new() -> Self {
            Self { bits: 0 }
        }
        pub fn bits(&self) -> u8 { self.bits }
        pub fn set_bits(&mut self, data: u8) { self.bits = data; }

        // Helper methods for individual flags
        pub fn nametable_addr(&self) -> u16 {
            match self.bits & 0x03 {
                0 => 0x2000, 1 => 0x2400, 2 => 0x2800, 3 => 0x2C00,
                _ => unreachable!(),
            }
        }
        pub fn vram_addr_increment(&self) -> u16 { if (self.bits & 0x04) == 0 { 1 } else { 32 } }
        pub fn sprite_pattern_addr(&self) -> u16 { if (self.bits & 0x08) == 0 { 0x0000 } else { 0x1000 } }
        pub fn background_pattern_addr(&self) -> u16 { if (self.bits & 0x10) == 0 { 0x0000 } else { 0x1000 } }
        pub fn sprite_size_large(&self) -> bool { (self.bits & 0x20) != 0 } // true = 8x16, false = 8x8
        pub fn master_slave_select(&self) -> bool { (self.bits & 0x40) != 0 } // Unused in NES
        pub fn generate_nmi(&self) -> bool { (self.bits & 0x80) != 0 } // True = Enable NMI on VBlank
    }

    // --- PPU Mask Register ($2001) ---
    #[derive(Debug, Default, Clone, Copy)]
    #[repr(transparent)]
    pub struct MaskRegister {
        bits: u8,
    }

    impl MaskRegister {
        pub fn new() -> Self { Self { bits: 0 } }
        pub fn bits(&self) -> u8 { self.bits }
        pub fn set_bits(&mut self, data: u8) { self.bits = data; }

        // Helper methods for individual flags
        pub fn grayscale(&self) -> bool { (self.bits & 0x01) != 0 }
        pub fn show_background_leftmost(&self) -> bool { (self.bits & 0x02) != 0 }
        pub fn show_sprites_leftmost(&self) -> bool { (self.bits & 0x04) != 0 }
        pub fn show_background(&self) -> bool { (self.bits & 0x08) != 0 }
        pub fn show_sprites(&self) -> bool { (self.bits & 0x10) != 0 }
        pub fn emphasize_red(&self) -> bool { (self.bits & 0x20) != 0 }
        pub fn emphasize_green(&self) -> bool { (self.bits & 0x40) != 0 }
        pub fn emphasize_blue(&self) -> bool { (self.bits & 0x80) != 0 }
    }

    // --- PPU Status Register ($2002) ---
    #[derive(Debug, Default, Clone, Copy)]
    #[repr(transparent)]
    pub struct StatusRegister {
        pub register: u8, // Keep the raw byte accessible
    }

    impl StatusRegister {
        pub fn new() -> Self { Self { register: 0 } }
        pub fn bits(&self) -> u8 { self.register }
        // pub fn set_bits(&mut self, data: u8) { self.register = data; } // Typically only bits 7-5 are written by PPU hardware

        // Getters for flags (read-only view)
        pub fn sprite_overflow(&self) -> bool { (self.register & 0x20) != 0 }
        pub fn sprite_zero_hit(&self) -> bool { (self.register & 0x40) != 0 }
        pub fn vblank_started(&self) -> bool { (self.register & 0x80) != 0 }

        // Setters used internally by PPU logic
        pub fn set_sprite_overflow(&mut self, value: bool) {
            if value { self.register |= 0x20; } else { self.register &= !0x20; }
        }
        pub fn set_sprite_zero_hit(&mut self, value: bool) {
            if value { self.register |= 0x40; } else { self.register &= !0x40; }
        }
        pub fn set_vblank_started(&mut self, value: bool) {
            if value { self.register |= 0x80; } else { self.register &= !0x80; }
        }
    }

    // --- PPU VRAM Address Register (Loopy's v and t) ---
    // Based on https://www.nesdev.org/wiki/PPU_scrolling#PPU_internal_registers
    #[derive(Debug, Default, Clone, Copy)]
    pub struct AddrRegister {
        address: u16, // 15-bit internal address ($0000-$7FFF)
                      // yyy NN YYYYY XXXXX
                      // ||| || ||||| +++++-- coarse X scroll (0-31)
                      // ||| || +++++-------- coarse Y scroll (0-31)
                      // ||| ++------------- nametable select (0-3)
                      // +++--------------- fine Y scroll (0-7)
    }

    impl AddrRegister {
        pub fn new() -> Self { Self { address: 0 } }
        pub fn addr(&self) -> u16 { self.address & 0x3FFF } // Effective 14-bit PPU address
        pub fn get(&self) -> u16 { self.address } // Internal 15-bit value

        pub fn set(&mut self, addr: u16) { self.address = addr & 0x7FFF; } // Mask to 15 bits

        pub fn increment(&mut self, inc: u16) {
            self.address = self.address.wrapping_add(inc) & 0x7FFF; // Increment and mask
        }

        pub fn copy_from(&mut self, other: &Self) {
            self.address = other.address;
        }

        // --- Field Accessors ---
        pub fn coarse_x(&self) -> u8 { (self.address & 0x001F) as u8 }
        pub fn set_coarse_x(&mut self, val: u8) { self.address = (self.address & !0x001F) | (val as u16 & 0x1F); }
        pub fn coarse_y(&self) -> u8 { ((self.address >> 5) & 0x001F) as u8 }
        pub fn set_coarse_y(&mut self, val: u8) { self.address = (self.address & !(0x1F << 5)) | ((val as u16 & 0x1F) << 5); }
        pub fn nametable_x(&self) -> u16 { (self.address >> 10) & 1 }
        pub fn nametable_y(&self) -> u16 { (self.address >> 11) & 1 }
        pub fn set_nametable_select(&mut self, val: u8) { self.address = (self.address & !(0x03 << 10)) | ((val as u16 & 0x03) << 10); }
        pub fn fine_y(&self) -> u16 { (self.address >> 12) & 0x0007 }
        pub fn set_fine_y(&mut self, val: u8) { self.address = (self.address & !(0x07 << 12)) | ((val as u16 & 0x07) << 12); }

        // --- Scrolling Logic Helpers (Based on nesdev wiki) ---
        pub fn inc_coarse_x(&mut self) -> bool {
            let coarse_x = self.coarse_x();
            if coarse_x == 31 {
                self.set_coarse_x(0); // Wrap coarse X to 0
                self.address ^= 0x0400; // Switch horizontal nametable
                true
            } else {
                self.set_coarse_x(coarse_x + 1);
                false
            }
        }

        pub fn inc_fine_y(&mut self) -> bool {
            let fine_y = self.fine_y();
            if fine_y < 7 {
                self.address += 0x1000; // Increment fine Y
                false
            } else {
                self.address &= !0x7000; // Fine Y = 0
                let coarse_y = self.coarse_y();
                if coarse_y == 29 {
                    self.set_coarse_y(0); // Coarse Y = 0
                    self.address ^= 0x0800; // Switch vertical nametable
                } else if coarse_y == 31 {
                    self.set_coarse_y(0); // Coarse Y = 0, wraps without switching nametable
                } else {
                    self.set_coarse_y(coarse_y + 1); // Increment coarse Y
                }
                true // Fine Y wrapped
            }
        }

        pub fn copy_horizontal_bits(&mut self, t: &AddrRegister) {
            self.address = (self.address & !0x041F) | (t.address & 0x041F); // Copy Nametable X and Coarse X
        }
        pub fn copy_vertical_bits(&mut self, t: &AddrRegister) {
            self.address = (self.address & !0x7BE0) | (t.address & 0x7BE0); // Copy Nametable Y, Coarse Y, Fine Y
        }

        // --- Register Write Helpers ---
        pub fn set_high_byte(&mut self, data: u8) {
             self.address = (self.address & 0x00FF) | ((data as u16 & 0x3F) << 8); // Mask to 6 bits, shift to high byte (bits 8-13)
         }
         pub fn set_low_byte(&mut self, data: u8) {
             self.address = (self.address & 0xFF00) | (data as u16); // Set low byte (bits 0-7)
         }

         // Helper for $2005 write (scroll)
         pub fn write_scroll(&mut self, data: u8, latch: &mut bool, fine_x: &mut u8, t: &mut AddrRegister) {
              if *latch { // First write (X)
                 t.set_coarse_x(data >> 3);
                 *fine_x = data & 0x07;
                 *latch = false;
             } else { // Second write (Y)
                 t.set_fine_y(data & 0x07);
                 t.set_coarse_y(data >> 3);
                 *latch = true;
             }
         }

          // Helper for $2006 write (address)
          pub fn write_addr(&mut self, data: u8, latch: &mut bool, t: &mut AddrRegister) {
               if *latch { // First write (High byte)
                 t.set_high_byte(data & 0x3F);
                 *latch = false;
             } else { // Second write (Low byte)
                 t.set_low_byte(data);
                 self.copy_from(t); // Copy t to v immediately on second write
                 *latch = true;
             }
          }
    }