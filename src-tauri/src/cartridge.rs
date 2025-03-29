use crate::Mirroring; // Mirroring enum is defined in main.rs

// Trait for Memory Mappers
// Added Send + Sync trait bounds for thread safety with Tauri State
pub trait Mapper: Send + Sync {
    fn read_prg(&self, addr: u16) -> u8;
    fn write_prg(&mut self, addr: u16, data: u8);
    fn read_chr(&self, addr: u16) -> u8;
    fn write_chr(&mut self, addr: u16, data: u8);
    fn mirroring(&self) -> Mirroring;
    // fn irq_state(&self) -> bool; // Add later if needed for specific mappers
    // fn irq_clear(&mut self);    // Add later if needed
    // fn scanline(&mut self);     // Add later if needed for scanline counters
}

// Mapper 0: NROM (No mapper logic, direct access)
struct Mapper0 {
    prg_banks: u8,
    chr_banks: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>, // Might be CHR RAM for some NROM carts
    mirroring: Mirroring,
}

impl Mapper for Mapper0 {
    fn read_prg(&self, addr: u16) -> u8 {
        // NROM maps $8000-$BFFF to first 16KB bank
        // and $C000-$FFFF to the last 16KB bank (or mirror of first if only 1 bank)
        let mut mapped_addr = addr as usize - 0x8000;
        if self.prg_banks == 1 && mapped_addr >= 0x4000 {
            // Mirror the first 16KB bank if only one exists
            mapped_addr %= 0x4000;
        }
        self.prg_rom.get(mapped_addr).copied().unwrap_or_else(|| {
             eprintln!("WARN: Read out of bounds PRG ROM access at {:04X} (mapped: {:04X})", addr, mapped_addr);
             0
        })
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        // Generally, PRG ROM is not writable for Mapper 0
         eprintln!("WARN: Attempted write to PRG ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
    }

    fn read_chr(&self, addr: u16) -> u8 {
        if self.chr_banks == 0 {
             // CHR RAMの場合
             // 実装されていない場合、0を返す（将来的にCHR RAMとして実装）
             return 0;
         } else {
             // 常にアドレス範囲を安全に処理する
             let addr = addr & 0x1FFF; // 8KB空間に制限
             if (addr as usize) < self.chr_rom.len() {
                 self.chr_rom[addr as usize]
             } else {
                 // 境界を超えた場合、エラーを表示（頻度を減らすために条件付き）
                 if addr % 0x100 == 0 { // 256バイト毎に1回だけ警告を表示
                     eprintln!("WARN: Read out of bounds CHR ROM access at {:04X}", addr);
                 }
                 // パターンテーブルが足りない場合は0を返す
                 0
             }
         }
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        if self.chr_banks == 0 {
            // Write to CHR RAM - TODO: Implement CHR RAM if needed
             eprintln!("WARN: Attempted CHR write to CHR RAM (Mapper 0) - not implemented yet. Addr: {:04X}, Data: {:02X}", addr, data);
        } else {
            // CHR ROM is generally not writable
             eprintln!("WARN: Attempted write to CHR ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

// Cartridge Structure
pub struct Cartridge {
    mapper_id: u8,
    prg_banks: u8,
    chr_banks: u8,
    // Use Box<dyn Mapper> to hold the specific mapper implementation
    mapper: Box<dyn Mapper>,
    mirroring: Mirroring, // Store mirroring determined at load time
}

impl Cartridge {
    pub fn new(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mapper_id: u8,
        mirroring_type: u8, // Usually from iNES header flags
    ) -> Result<Self, String> {

        let prg_banks = (prg_rom.len() / 16384) as u8; // 16KB banks
        let chr_banks = (chr_rom.len() / 8192) as u8;  // 8KB banks

        // Determine Mirroring mode from header flag
        let mirroring = if (mirroring_type & 0x08) != 0 {
            Mirroring::FourScreen
        } else if (mirroring_type & 0x01) != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        // Instantiate the correct mapper based on mapper_id
        let mapper: Box<dyn Mapper> = match mapper_id {
            0 => {
                // Create Mapper 0 instance
                 let chr_data = if chr_banks == 0 { Vec::new() } else { chr_rom }; // Handle CHR RAM case later maybe
                Box::new(Mapper0 {
                    prg_banks,
                    chr_banks,
                    prg_rom,
                    chr_rom: chr_data,
                    mirroring,
                })
            }
            // TODO: Add other mappers (1, 2, 3, 4, etc.) here
            _ => {
                return Err(format!("Unsupported mapper ID: {}", mapper_id));
            }
        };

        println!(
            "Cartridge loaded: Mapper {}, PRG Banks: {}, CHR Banks: {}, Mirroring: {:?}",
            mapper_id, prg_banks, chr_banks, mirroring
        );

        Ok(Self {
            mapper_id,
            prg_banks,
            chr_banks,
            mapper, // Store the boxed mapper
            mirroring, // Store determined mirroring
        })
    }

    // Read/Write methods delegate to the contained mapper
    pub fn read_prg(&self, addr: u16) -> u8 {
        self.mapper.read_prg(addr)
    }

    pub fn write_prg(&mut self, addr: u16, data: u8) {
        self.mapper.write_prg(addr, data);
    }

    pub fn read_chr(&self, addr: u16) -> u8 {
        self.mapper.read_chr(addr)
    }

    pub fn write_chr(&mut self, addr: u16, data: u8) {
        self.mapper.write_chr(addr, data);
    }

    pub fn mirror_mode(&self) -> Mirroring {
        self.mirroring // Return stored mirroring mode
    }
} 