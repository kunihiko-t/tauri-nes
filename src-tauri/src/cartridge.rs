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
    chr_rom: Vec<u8>, // Used if chr_banks > 0
    chr_ram: Vec<u8>, // Added for CHR RAM support (8KB)
    mirroring: Mirroring,
    // BG切り替えスイッチ対応
    bg_switch_enabled: bool,
    bg_bank_selected: u8,
}

impl Mapper for Mapper0 {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRGメモリは0x8000-0xFFFFの範囲にマッピングされるべき
        if addr < 0x8000 {
            // 一部のゲームは低アドレス領域も使用することがある
            // 警告を出さずに0を返す
            return 0;
        }

        // すべての$8000以上のアクセスはこちらで処理
        let mapped_addr = if self.prg_banks == 1 {
            // NROM-128 (16KB PRG): $8000-$BFFF maps to the 16KB ROM, mirrored at $C000-$FFFF
            (addr & 0x3FFF) as usize // Mask to 14 bits (16KB range)
        } else {
            // NROM-256 (32KB PRG): $8000-$FFFF maps directly to the 32KB ROM
            (addr & 0x7FFF) as usize // Mask to 15 bits (32KB range)
        };
        
        // Read from PRG ROM
        if mapped_addr < self.prg_rom.len() {
            self.prg_rom[mapped_addr]
        } else {
            // Handle potential out-of-bounds read, although masking should prevent this
             // Limit log spam
            if addr % 0x100 == 0 {
                 eprintln!("WARN: Read out of bounds PRG ROM access at {:04X} (Mapped: {}, Size: {})", addr, mapped_addr, self.prg_rom.len());
            }
            0xFF // Return 0xFF (often represents open bus behavior)
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        // マッパー0は通常PRG ROMに書き込めないが、特殊な機能を追加
        // BG切り替えスイッチ機能の実装
        // if addr >= 0x8000 && addr <= 0x8FFF { // <<< この if ブロック全体をコメントアウト
        //     // $8000-$8FFFへの書き込みを特殊なマッパーレジスタとして扱う
        //     if data & 0x80 != 0 {
        //         // BG切り替えスイッチ有効化
        //         self.bg_switch_enabled = true;
        //         self.bg_bank_selected = data & 0x03; // 下位2ビットでバンク選択
        //         println!("Mapper 0: BG Switch enabled, bank: {}", self.bg_bank_selected);
        //     } else {
        //         // 通常はPRG ROMに書き込めない
        //         // eprintln!("WARN: Attempted write to PRG ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
        //     }
        // } else { // <<< この else と対応する括弧も
        //     // 通常はPRG ROMに書き込めない
        //     // eprintln!("WARN: Attempted write to PRG ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
        // } // <<< ここまでコメントアウト
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let original_addr = addr; // 元のアドレスをログ用に保持
        let addr = addr & 0x1FFF; // Ensure address is within 8KB range
        
        if self.chr_banks == 0 {
            // CHR RAM read
            let index = addr as usize;
            if index < self.chr_ram.len() {
                // ★★★ CHR RAM 読み込みログ ★★★
                // Limit log spam, e.g., log only first few addresses or specific tiles if needed
                // if index < 0x10 || (index >= 0x1000 && index < 0x1010) {
                //    println!("--- CHR RAM Read: OrigAddr:{:04X} Addr:{:04X} Index:{} Size:{} -> Data:{:02X} ---",
                //             original_addr, addr, index, self.chr_ram.len(), self.chr_ram[index]);
                // }
                // ★★★ ここまで ★★★
                self.chr_ram[index]
            } else {
                eprintln!("WARN: Read out of bounds CHR RAM access at {:04X} (Index: {}, Size: {})", addr, index, self.chr_ram.len());
                0
            }
        } else {
            // CHR ROM read with BG切り替えスイッチ対応
            // if self.bg_switch_enabled && addr >= 0x1000 { // ★ 特殊な BG 切り替え機能
            //     // パターンテーブル1 ($1000-$1FFF) のアクセス時、バンク切り替え
            //     let bank_offset = self.bg_bank_selected as usize * 0x1000; // ★ 4KB バンクと仮定している？
            //     let offset_addr = addr as usize - 0x1000;
            //     let final_index = bank_offset + offset_addr;
            //
            //     if final_index < self.chr_rom.len() {
            //          // ★★★ CHR ROM Bank Read ログ ★★★
            //         // Limit log spam
            //         // if addr % 0x10 == 0 {
            //         //    println!("--- CHR ROM Bank Read: OrigAddr:{:04X} Addr:{:04X} Bank:{} Index:{} Size:{} -> Data:{:02X} ---",
            //         //            original_addr, addr, self.bg_bank_selected, final_index, self.chr_rom.len(), self.chr_rom[final_index]);
            //         // }
            //         // ★★★ ここまで ★★★
            //         return self.chr_rom[final_index];
            //     } else {
            //         if addr % 0x100 == 0 { // Limit log spam
            //             eprintln!("WARN: Read out of bounds CHR ROM bank access at {:04X} (Bank: {}, Index: {}, Size: {})",
            //                 addr, self.bg_bank_selected, final_index, self.chr_rom.len());
            //         }
            //         return 0;
            //     }
            // } else {
            // --- ここまで ---
                // 通常のCHR ROMアクセス (BG Switch disabled or addr < 0x1000)
                let index = addr as usize;
                if index < self.chr_rom.len() {
                     // ★★★ CHR ROM Read (Normal) ログ ★★★
                     // Limit log spam
                     // if addr % 0x10 == 0 {
                     //    println!("--- CHR ROM Read (Normal/BG Disabled/<0x1000): OrigAddr:{:04X} Addr:{:04X} Index:{} Size:{} -> Data:{:02X} ---",
                     //             original_addr, addr, index, self.chr_rom.len(), self.chr_rom[index]);
                    // }
                     // ★★★ ここまで ★★★
                    self.chr_rom[index]
                } else {
                     if addr % 0x100 == 0 { // Limit log spam
                        eprintln!("WARN: Read out of bounds CHR ROM access at {:04X} (Index: {}, Size: {})", addr, index, self.chr_rom.len());
                     }
                    0
                }
            // --- BG Switch を無効化 ---
            // }
            // --- ここまで ---
        }
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        let original_addr = addr; // 元のアドレスをログ用に保持
        let addr = addr & 0x1FFF; // Ensure address is within 8KB range
        if self.chr_banks == 0 {
            // CHR RAM write
            let index = addr as usize;
            if index < self.chr_ram.len() {
                // ★★★ CHR RAM 書き込みログ ★★★
                // Limit log spam if necessary
                 println!("--- CHR RAM Write: OrigAddr:{:04X} Addr:{:04X} Index:{} Size:{} Data:{:02X} ---",
                          original_addr, addr, index, self.chr_ram.len(), data);
                // ★★★ ここまで ★★★
                self.chr_ram[index] = data;
            } else {
                eprintln!("WARN: Write out of bounds CHR RAM access at {:04X} (Index: {}, Size: {})", addr, index, self.chr_ram.len());
            }
        } else {
            // CHR ROM is generally not writable
            // eprintln!("WARN: Attempted write to CHR ROM (Mapper 0) at OrigAddr:{:04X} Addr:{:04X} with data {:02X}", original_addr, addr, data); // Comment out the warning
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
                let mut chr_ram = vec![0u8; 0]; // Initialize as empty
                if chr_banks == 0 {
                     println!("Mapper 0: Using 8KB CHR RAM");
                    chr_ram = vec![0u8; 8192]; // Allocate 8KB if no CHR ROM
                }
                let chr_data = if chr_banks == 0 { Vec::new() } else { chr_rom }; // Pass empty Vec if CHR RAM

                Box::new(Mapper0 {
                    prg_banks,
                    chr_banks,
                    prg_rom,
                    chr_rom: chr_data,
                    chr_ram, // Add chr_ram field
                    mirroring,
                    // BG切り替えスイッチ対応
                    bg_switch_enabled: false,
                    bg_bank_selected: 0,
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

    pub fn get_mirroring(&self) -> Mirroring {
        self.mirroring
    }

    pub fn get_mapper_id(&self) -> u8 {
        self.mapper_id
    }
} 