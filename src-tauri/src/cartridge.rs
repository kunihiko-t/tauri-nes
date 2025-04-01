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

        // 特別なケース: ベクタアドレスのアクセス時にデバッグログを出力
        if addr >= 0xFFFA && addr <= 0xFFFF {
            // NROM maps $8000-$BFFF to first 16KB bank
            // and $C000-$FFFF to the last 16KB bank (or mirror of first if only 1 bank)
            let mapped_addr = if self.prg_banks == 1 {
                // 16KBバンクが1つしかない場合: すべてをミラーリング
                ((addr - 0x8000) % 0x4000) as usize
            } else { 
                // 32KBバンクの場合: 直接マッピング
                (addr - 0x8000) as usize
            };
            
            if mapped_addr < self.prg_rom.len() {
                let value = self.prg_rom[mapped_addr];
                
                // デバッグログを出力
                match addr {
                    0xFFFA | 0xFFFB => {
                        println!("NMI vector read at ${:04X}: ${:02X} (ROM addr: ${:04X})", 
                            addr, value, mapped_addr);
                    },
                    0xFFFC | 0xFFFD => {
                        println!("Reset vector read at ${:04X}: ${:02X} (ROM addr: ${:04X})", 
                            addr, value, mapped_addr);
                    },
                    0xFFFE | 0xFFFF => {
                        println!("IRQ vector read at ${:04X}: ${:02X} (ROM addr: ${:04X})", 
                            addr, value, mapped_addr);
                    },
                    _ => {}
                }
                
                return value;
            } else {
                return 0;
            }
        }

        // 通常のアクセス処理（ベクタアドレス以外）
        let mapped_addr = if self.prg_banks == 1 {
            // 16KBバンクが1つしかない場合: すべてをミラーリング
            ((addr - 0x8000) % 0x4000) as usize
        } else { 
            // 32KBバンクの場合: 直接マッピング
            (addr - 0x8000) as usize
        };
        
        // アドレスが範囲内かチェック
        if mapped_addr < self.prg_rom.len() {
            self.prg_rom[mapped_addr]
        } else {
            0
        }
    }

    fn write_prg(&mut self, addr: u16, data: u8) {
        // マッパー0は通常PRG ROMに書き込めないが、特殊な機能を追加
        // BG切り替えスイッチ機能の実装
        if addr >= 0x8000 && addr <= 0x8FFF {
            // $8000-$8FFFへの書き込みを特殊なマッパーレジスタとして扱う
            if data & 0x80 != 0 {
                // BG切り替えスイッチ有効化
                self.bg_switch_enabled = true;
                self.bg_bank_selected = data & 0x03; // 下位2ビットでバンク選択
                println!("Mapper 0: BG Switch enabled, bank: {}", self.bg_bank_selected);
            } else {
                // 通常はPRG ROMに書き込めない
                eprintln!("WARN: Attempted write to PRG ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
            }
        } else {
            // 通常はPRG ROMに書き込めない
            eprintln!("WARN: Attempted write to PRG ROM (Mapper 0) at {:04X} with data {:02X}", addr, data);
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF; // Ensure address is within 8KB range
        
        if self.chr_banks == 0 {
            // CHR RAM read
            if (addr as usize) < self.chr_ram.len() {
                self.chr_ram[addr as usize]
            } else {
                eprintln!("WARN: Read out of bounds CHR RAM access at {:04X}", addr);
                0
            }
        } else {
            // CHR ROM read with BG切り替えスイッチ対応
            if self.bg_switch_enabled && addr >= 0x1000 {
                // パターンテーブル1のアクセス時、バンク切り替え
                let bank_offset = self.bg_bank_selected as usize * 0x1000;
                let offset_addr = addr as usize - 0x1000;
                
                if bank_offset + offset_addr < self.chr_rom.len() {
                    return self.chr_rom[bank_offset + offset_addr];
                } else {
                    if addr % 0x100 == 0 {
                        eprintln!("WARN: Read out of bounds CHR ROM bank access at {:04X}, bank: {}", 
                            addr, self.bg_bank_selected);
                    }
                    return 0;
                }
            } else {
                // 通常のCHR ROMアクセス
                if (addr as usize) < self.chr_rom.len() {
                    self.chr_rom[addr as usize]
                } else {
                    if addr % 0x100 == 0 {
                        eprintln!("WARN: Read out of bounds CHR ROM access at {:04X}", addr);
                    }
                    0
                }
            }
        }
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x1FFF; // Ensure address is within 8KB range
        if self.chr_banks == 0 {
            // CHR RAM write
            if (addr as usize) < self.chr_ram.len() {
                // ★★★ CHR RAM 書き込みログ ★★★
                if addr < 0x10 || (addr >= 0x1000 && addr < 0x1010) { // Limit output
                    println!("--- CHR RAM Write: Addr:{:04X} Data:{:02X} ---", addr, data);
                }
                // ★★★ ここまで ★★★
                self.chr_ram[addr as usize] = data;
            } else {
                eprintln!("WARN: Write out of bounds CHR RAM access at {:04X}", addr);
            }
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
} 