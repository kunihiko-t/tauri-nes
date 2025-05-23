use serde::Serialize; // Import Serialize
use crate::Mirroring; // Ensure Mirroring is imported from crate root (main.rs)
use crate::bus::BusAccess;             // Ensure Bus is imported
use crate::registers::{AddrRegister, ControlRegister, MaskRegister, StatusRegister}; // Assuming registers module exists
// use std::cell::RefCell; // Remove unused import
// use std::rc::Rc; // Remove unused import
// use std::cell::RefCell; // Remove unused import

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
// const CYCLES_PER_SCANLINE: u64 = 341; // Remove or keep if needed elsewhere
// const SCANLINES_PER_FRAME: u64 = 262;
// const STATUS_VBLANK: u8 = 0x80; // Use StatusRegister constants
// const CTRL_VRAM_INCREMENT: u8 = 0x04; // Use ControlRegister constants

pub struct Ppu {
    // CPU接続
    pub nmi_line_low: bool,         // NMI割り込みライン
    
    // PPUレジスタ
    pub ctrl: ControlRegister,      // $2000 
    pub mask: MaskRegister,         // $2001
    pub status: StatusRegister,     // $2002
    
    // アドレス関連
    pub vram_addr: AddrRegister,     // 現在のVRAMアドレス
    pub temp_vram_addr: AddrRegister, // 一時VRAMアドレス
    pub fine_x_scroll: u8,            // 水平方向の微調整スクロール
    pub address_latch_low: bool,      // アドレスバイトラッチのフラグ
    pub data_buffer: u8,              // PPUデータバッファ
    
    // OAM関連
    pub oam_addr: u8,               // OAMアドレス
    pub oam_data: [u8; 256],        // OAMデータ (64スプライト × 4バイト)
    
    // PPUタイミング
    pub cycle: usize,                // 現在のピクセルサイクル (0-340)
    pub scanline: isize,             // 現在のスキャンライン (-1 to 260)
    pub frame_complete: bool,        // フレーム完了フラグ
    pub frame_counter: u64,          // フレームカウンタ
    
    // メモリ
    pub palette_ram: [u8; 32],       // パレットRAM
    pub vram: [u8; 2048],            // 8KBのVRAM
    pub chr_ram: [u8; 8192],         // 8KBのCHR-RAM
    pub mirroring: Mirroring,        // ミラーリングモード
    
    // 背景レンダリング用レジスタ
    pub bg_next_tile_id: u8,        // 次に描画するタイルのID
    pub bg_next_tile_attr: u8,       // 次に描画するタイルの属性
    pub bg_next_tile_lsb: u8,        // 次に描画するタイルの下位ビット
    pub bg_next_tile_msb: u8,        // 次に描画するタイルの上位ビット
    
    // 背景シフトレジスタ
    pub bg_shifter_pattern_lo: u16,  // 背景パターンシフトレジスタ（下位）
    pub bg_shifter_pattern_hi: u16,  // 背景パターンシフトレジスタ（上位）
    pub bg_shifter_attrib_lo: u16,   // 背景属性シフトレジスタ（下位）
    pub bg_shifter_attrib_hi: u16,   // 背景属性シフトレジスタ（上位）
    
    // フレームデータ
    pub frame: FrameData,           // 現在のフレームデータ
}

impl Ppu {
    pub fn new() -> Self {
        let mut ppu = Self {
            nmi_line_low: true,
            ctrl: ControlRegister::new(),
            mask: MaskRegister::new(),
            status: StatusRegister::new(),
            vram_addr: AddrRegister::new(),
            temp_vram_addr: AddrRegister::new(),
            fine_x_scroll: 0,
            address_latch_low: true,
            data_buffer: 0,
            oam_addr: 0,
            oam_data: [0; 256],
            cycle: 0,
            scanline: -1,
            frame_complete: false,
            frame_counter: 0,
            palette_ram: [0; 32],
            vram: [0; 2048],
            chr_ram: [0; 8192],
            mirroring: Mirroring::Horizontal,
            bg_next_tile_id: 0,
            bg_next_tile_attr: 0,
            bg_next_tile_lsb: 0,
            bg_next_tile_msb: 0,
            bg_shifter_pattern_lo: 0,
            bg_shifter_pattern_hi: 0,
            bg_shifter_attrib_lo: 0,
            bg_shifter_attrib_hi: 0,
            frame: FrameData::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        };

        ppu.reset();
        ppu
    }

    pub fn reset(&mut self) {
        // println!("PPU Reset started...");
        
        // PPUの内部状態をリセット
        self.cycle = 0;
        self.scanline = -1; // Start at pre-render scanline
        self.frame_complete = false;
        self.frame_counter = 0;
        self.nmi_line_low = true;
        self.address_latch_low = true;
        self.fine_x_scroll = 0;
        self.data_buffer = 0;
        self.oam_addr = 0;
        
        // 背景レンダリング用レジスタを初期化
        self.bg_next_tile_id = 0;
        self.bg_next_tile_attr = 0;
        self.bg_next_tile_lsb = 0;
        self.bg_next_tile_msb = 0;
        
        // 背景シフトレジスタを初期化
        self.bg_shifter_pattern_lo = 0;
        self.bg_shifter_pattern_hi = 0;
        self.bg_shifter_attrib_lo = 0;
        self.bg_shifter_attrib_hi = 0;

        // PPUレジスタをリセット
        self.status.register = 0x00;
        self.mask.set_bits(0x00);  // 表示無効化
        self.ctrl.set_bits(0xA0);  // NMI有効化とバックグラウンドパターンテーブル1を選択

        // VRAMアドレスレジスタをリセット
        self.vram_addr.set(0x0000);
        self.temp_vram_addr.set(0x0000);

        // フレームバッファをクリア
        for i in 0..self.frame.pixels.len() {
            self.frame.pixels[i] = 0;
        }
        
        // PPUパレットを初期化 - 青系の色のセット
        self.palette_ram[0] = 0x0F; // 背景色 (黒)
        
        // パレット1（青系）
        self.palette_ram[1] = 0x01; // ダークブルー
        self.palette_ram[2] = 0x11; // ミディアムブルー 
        self.palette_ram[3] = 0x21; // ライトブルー
        
        // パレット2（水色系）
        self.palette_ram[5] = 0x0C; // ダーク水色
        self.palette_ram[6] = 0x1C; // ミディアム水色
        self.palette_ram[7] = 0x2C; // ライト水色
        
        // パレット3（紫系）
        self.palette_ram[9] = 0x13; // ダーク紫
        self.palette_ram[10] = 0x23; // ミディアム紫
        self.palette_ram[11] = 0x33; // ライト紫
        
        // パレット4（シアン系）
        self.palette_ram[13] = 0x1A; // ダークシアン
        self.palette_ram[14] = 0x2A; // ミディアムシアン
        self.palette_ram[15] = 0x3A; // ライトシアン
        
        // ミラーリングの処理
        self.palette_ram[0x10] = self.palette_ram[0];
        self.palette_ram[0x14] = self.palette_ram[0x04];
        self.palette_ram[0x18] = self.palette_ram[0x08];
        self.palette_ram[0x1C] = self.palette_ram[0x0C];
        
        // テスト用のCHR-RAMデータをセットアップ（パターンテーブルデータをシミュレート）
        self.init_test_chr_data();
        
        // 初期状態のフラグを設定
        self.ctrl.set_bits(0xA0);  // NMI有効化とバックグラウンドパターンテーブル1を選択
        // マスクレジスタを更新して背景とスプライトを表示
        self.mask.set_bits(0x1E);  // 背景とスプライトを有効化（0x1EはBGとスプライト両方有効）
        
        // println!("PPU Reset complete. PPUCTRL=${:02X}, PPUMASK=${:02X}", self.ctrl.bits(), self.mask.bits());
    }

    // テスト用のパターンを描画
    pub fn init_test_pattern(&mut self) {
        // フレームデータが初期化されているか確認
        if self.frame.pixels.len() != SCREEN_WIDTH * SCREEN_HEIGHT * 4 {
            return;
        }
        
        // 静的変数でログの出力頻度を制限
        static mut LOG_COUNTER: u32 = 0;
        static mut INIT_LOGS_DONE: bool = false;
        let should_log = unsafe {
            LOG_COUNTER += 1;
            if !INIT_LOGS_DONE && LOG_COUNTER > 10 {
                INIT_LOGS_DONE = true;
                true
            } else {
                LOG_COUNTER % 500 == 0 // 500回に1回だけログを出力
            }
        };
        
        if should_log {
            // println!("初期テストパターンを描画中...");
        }
        
        // テストモード用のグラデーションパターンを描画
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let idx = (y * SCREEN_WIDTH + x) * 4;
                
                // グラデーションパターン（位置に基づく色変化）
                let r = ((x as f32 / SCREEN_WIDTH as f32) * 255.0) as u8;
                let g = ((y as f32 / SCREEN_HEIGHT as f32) * 255.0) as u8;
                let b = (((x + y) as f32 / (SCREEN_WIDTH + SCREEN_HEIGHT) as f32) * 255.0) as u8;
                
                // 中央に白い十字線を描画
                let center_x = SCREEN_WIDTH / 2;
                let center_y = SCREEN_HEIGHT / 2;
                
                if (x == center_x || y == center_y) || 
                   // 枠線を描画
                   (x < 2 || x >= SCREEN_WIDTH - 2 || y < 2 || y >= SCREEN_HEIGHT - 2) {
                    self.frame.pixels[idx] = 255;     // R
                    self.frame.pixels[idx + 1] = 255; // G
                    self.frame.pixels[idx + 2] = 255; // B
                } else {
                    self.frame.pixels[idx] = r;     // R
                    self.frame.pixels[idx + 1] = g; // G
                    self.frame.pixels[idx + 2] = b; // B
                }
                self.frame.pixels[idx + 3] = 255;   // A (不透明)
            }
        }
        
        if should_log {
            // println!("初期テストパターン描画完了！");
        }
    }

    // CHR-RAMからタイルを描画する補助メソッド
    fn draw_chr_tile(&mut self, x_pos: usize, y_pos: usize, tile_index: usize, palette: usize) {
        let base_addr = tile_index * 16; // 各タイルは16バイト
        
        for y in 0..8 {
            let low_byte = self.chr_ram[base_addr + y];
            let high_byte = self.chr_ram[base_addr + y + 8];
            
            for x in 0..8 {
                let bit = 7 - x; // ビット位置は反転している
                let low_bit = (low_byte >> bit) & 0x01;
                let high_bit = (high_byte >> bit) & 0x01;
                let pixel_value = (high_bit << 1) | low_bit;
                
                if pixel_value > 0 { // 0以外のピクセルのみ描画（0は透明）
                    let color_addr = (palette * 4 + pixel_value as usize) % 32;
                    let color_idx = self.palette_ram[color_addr] as usize;
                    let (r, g, b) = NES_PALETTE[color_idx & 0x3F];
                    
                    let screen_x = x_pos + x;
                    let screen_y = y_pos + y;
                    
                    if screen_x < SCREEN_WIDTH && screen_y < SCREEN_HEIGHT {
                        let idx = (screen_y * SCREEN_WIDTH + screen_x) * 4;
                        if idx + 3 < self.frame.pixels.len() {
                            self.frame.pixels[idx] = r;
                            self.frame.pixels[idx + 1] = g;
                            self.frame.pixels[idx + 2] = b;
                            self.frame.pixels[idx + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    // テスト用のCHR-RAMデータを初期化
    fn init_test_chr_data(&mut self) {
        // シンプルなパターンデータを作成（8x8タイルのテスト用パターン）
        
        // パターン0: 中央に点がある
        let pattern0_lo = [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
            0b00000000,
            0b00000000,
        ];
        
        let pattern0_hi = [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00011000,
            0b00011000,
            0b00000000,
            0b00000000,
            0b00000000,
        ];
        
        // パターン1: 格子柄
        let pattern1_lo = [
            0b10101010,
            0b00000000,
            0b10101010,
            0b00000000,
            0b10101010,
            0b00000000,
            0b10101010,
            0b00000000,
        ];
        
        let pattern1_hi = [
            0b00000000,
            0b10101010,
            0b00000000,
            0b10101010,
            0b00000000,
            0b10101010,
            0b00000000,
            0b10101010,
        ];
        
        // パターン2: 外枠
        let pattern2_lo = [
            0b11111111,
            0b10000001,
            0b10000001,
            0b10000001,
            0b10000001,
            0b10000001,
            0b10000001,
            0b11111111,
        ];
        
        let pattern2_hi = [
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
            0b00000000,
        ];
        
        // テストパターンをCHR-RAMに書き込む
        // パターン0
        for i in 0..8 {
            self.chr_ram[i] = pattern0_lo[i];
            self.chr_ram[i + 8] = pattern0_hi[i];
        }
        
        // パターン1
        for i in 0..8 {
            self.chr_ram[16 + i] = pattern1_lo[i];
            self.chr_ram[16 + i + 8] = pattern1_hi[i];
        }
        
        // パターン2
        for i in 0..8 {
            self.chr_ram[32 + i] = pattern2_lo[i];
            self.chr_ram[32 + i + 8] = pattern2_hi[i];
        }
        
        // ネームテーブルにテストタイルを配置（画面左上の数タイル）
        for i in 0..16 {
            self.vram[i] = (i % 3) as u8;  // パターン0,1,2を繰り返す
        }
        
        // 属性テーブルも初期化
        self.vram[0x3C0] = 0x55;  // パレット1をデフォルトに
        
        // println!("Test CHR-RAM and nametable data initialized");
    }

    // PPUを1サイクル進めるメソッド
    pub fn step_cycle(&mut self, bus: &impl BusAccess) -> bool {
        // Log state at the beginning of the cycle (less frequently)
        if self.cycle == 0 && self.scanline % 16 == 0 { // Log every 16 scanlines at cycle 0
             // println!("[Cycle Start] Scanline: {}, Cycle: {}, v: {:04X}, t: {:04X}",
                      // self.scanline, self.cycle, self.vram_addr.get(), self.temp_vram_addr.get());
        }

        let rendering_enabled = self.mask.show_background() || self.mask.show_sprites();

        // Actions for Pre-render scanline (-1) AND visible scanlines (0-239)
        // These are common operations related to fetching and VRAM address updates
        if self.scanline == -1 || (0..=239).contains(&self.scanline) {
            // --- Background Processing Cycles (1-256) ---
            if (1..=256).contains(&self.cycle) {
                if self.scanline != -1 { // Only render pixels on visible scanlines
                    self.render_pixel();
                }

                if rendering_enabled { // Shifting happens even on pre-render if rendering would be on
                    // Shift background registers (happens on cycles 2-257 according to wiki)
                    // We shift *after* rendering the current pixel.
                    if self.mask.show_background() { // Only shift if background is enabled
                        self.bg_shifter_pattern_lo <<= 1;
                        self.bg_shifter_pattern_hi <<= 1;
                        self.bg_shifter_attrib_lo <<= 1;
                        self.bg_shifter_attrib_hi <<= 1;
                    }
                }
                
                // Perform fetches (this logic needs to run on pre-render too if rendering_enabled)
                if rendering_enabled {
                    match self.cycle % 8 {
                        1 => { // Fetch Nametable byte for the *next* tile
                            self.load_background_shifters();
                            let nt_addr = 0x2000 | (self.vram_addr.get() & 0x0FFF);
                            let mirrored_nt_addr = self.mirror_vram_addr(nt_addr, self.mirroring) as u16;
                            self.bg_next_tile_id = bus.ppu_read_vram(mirrored_nt_addr);
                        }
                        3 => { // Fetch Attribute Table byte
                            let nametable_select = (self.vram_addr.nametable_y() << 1) | self.vram_addr.nametable_x();
                            let attr_addr: u16 = 0x23C0 | (nametable_select << 10)
                                           | (((self.vram_addr.coarse_y() >> 2) as u16) << 3)
                                           | ((self.vram_addr.coarse_x() >> 2) as u16);
                            let mirrored_attr_addr = self.mirror_vram_addr(attr_addr, self.mirroring) as u16;
                            let attr_byte = bus.ppu_read_vram(mirrored_attr_addr);
                            let shift = ((self.vram_addr.coarse_y() & 0x02) << 1) | (self.vram_addr.coarse_x() & 0x02);
                            self.bg_next_tile_attr = (attr_byte >> shift) & 0x03;
                        }
                        5 => { // Fetch Pattern Table Low byte
                            let pattern_table_base = self.ctrl.background_pattern_addr();
                            let tile_addr = pattern_table_base + (self.bg_next_tile_id as u16 * 16);
                            let fine_y = self.vram_addr.fine_y() as u16;
                            let addr = tile_addr + fine_y;
                            self.bg_next_tile_lsb = bus.ppu_read_vram(addr);
                        }
                        7 => { // Fetch Pattern Table High byte
                            let pattern_table_base = self.ctrl.background_pattern_addr();
                            let tile_addr = pattern_table_base + (self.bg_next_tile_id as u16 * 16);
                            let fine_y = self.vram_addr.fine_y() as u16;
                            let pattern_addr_high = tile_addr + fine_y + 8;
                            self.bg_next_tile_msb = bus.ppu_read_vram(pattern_addr_high);
                        }
                        0 => { // End of 8-cycle fetch period (on cycles 8, 16, ..., 256)
                            self.increment_scroll_x();
                        }
                        _ => {} // Cycles 2, 4, 6: Idle background fetch cycles
                    }
                }
            }

            // --- Sprite Processing Cycles (257-320) ---
            // TODO: Implement sprite evaluation for visible scanlines, 
            //       and dummy sprite fetches for pre-render if necessary.
            //       OAM Addr reset to 0 typically happens during cycles 257-320 of pre-render scanline.
            //       If self.scanline == -1 && self.cycle >= 257 && self.cycle <= 320 { self.oam_addr = 0; }

            // Reset horizontal VRAM address components at cycle 257
            if self.cycle == 257 && rendering_enabled {
                self.transfer_address_x();
                // if self.scanline == -1 { self.oam_addr = 0; } // Example of OAM addr reset on pre-render
            }

            // Background Fetch Cycles for Next Scanline's First Two Tiles (321-336)
            if (321..=336).contains(&self.cycle) && rendering_enabled {
                 match self.cycle % 8 {
                     1 => { 
                         self.load_background_shifters();
                         let nt_addr = 0x2000 | (self.vram_addr.get() & 0x0FFF);
                         let mirrored_nt_addr = self.mirror_vram_addr(nt_addr, self.mirroring) as u16;
                         self.bg_next_tile_id = bus.ppu_read_vram(mirrored_nt_addr);
                     }
                     3 => { // Fetch AT byte
                         let nametable_select = (self.vram_addr.nametable_y() << 1) | self.vram_addr.nametable_x();
                         let attr_addr: u16 = 0x23C0 | (nametable_select << 10)
                                        | (((self.vram_addr.coarse_y() >> 2) as u16) << 3)
                                        | ((self.vram_addr.coarse_x() >> 2) as u16);
                         let mirrored_attr_addr = self.mirror_vram_addr(attr_addr, self.mirroring) as u16;
                         let attr_byte = bus.ppu_read_vram(mirrored_attr_addr);
                         let shift = ((self.vram_addr.coarse_y() & 0x02) << 1) | (self.vram_addr.coarse_x() & 0x02);
                         self.bg_next_tile_attr = (attr_byte >> shift) & 0x03;
                     }
                     5 => { // Fetch PT Low byte
                         let pattern_table_base = self.ctrl.background_pattern_addr() as u16;
                         let pattern_addr_low = pattern_table_base
                             + (self.bg_next_tile_id as u16 * 16)
                             + self.vram_addr.fine_y() as u16;
                         self.bg_next_tile_lsb = bus.ppu_read_vram(pattern_addr_low);
                     }
                     7 => { // Fetch PT High byte
                         let pattern_table_base = self.ctrl.background_pattern_addr() as u16;
                         let pattern_addr_high = pattern_table_base
                             + (self.bg_next_tile_id as u16 * 16)
                             + self.vram_addr.fine_y() as u16
                             + 8;
                         self.bg_next_tile_msb = bus.ppu_read_vram(pattern_addr_high);
                     }
                     0 => { // End of 8-cycle fetch (cycles 328, 336)
                         self.increment_scroll_x();
                     }
                     _ => {}
                 }
            }

            // Increment vertical VRAM address at the end of cycle 256
            if self.cycle == 256 && rendering_enabled {
                self.increment_scroll_y();
            }
        } // End of common logic for scanlines -1 and 0-239

        // Specific Pre-render Scanline (-1) actions (or scanline 261, which is an alias for pre-render)
        if self.scanline == -1 || self.scanline == 261 { // scanline 261 is effectively the pre-render scanline
            if self.cycle == 1 {
                self.status.register &= !(StatusRegister::VBLANK_STARTED | StatusRegister::SPRITE_OVERFLOW | StatusRegister::SPRITE_ZERO_HIT);
                // self.nmi_line_low = true; // NMI is cleared by reading $2002 or at end of VBlank
            }
            // Vertical address transfer from t to v
            if self.cycle >= 280 && self.cycle <= 304 && rendering_enabled {
                self.transfer_address_y();
            }
            // OAM Addr reset to 0 typically happens during cycles 257-320 of pre-render scanline
            // This should be part of sprite processing logic for next scanline.
            // For now, as a placeholder if not handled by sprite logic:
            if self.cycle >= 257 && self.cycle <= 320 { // Placeholder for OAM addr reset timing
                 // self.oam_addr = 0; // This would be part of sprite evaluation for next line.
            }
        }
        
        // --- Post-render Scanline (240) ---
        if self.scanline == 240 { // Note: scanline 261 is the pre-render scanline, not part of post-render.
            // PPU is idle, CPU runs freely
        }

        // --- VBlank Scanlines (241-260) ---
        if self.scanline == 241 {
            if self.cycle == 1 {
                self.status.set_vblank_started(true);
                if self.ctrl.generate_nmi() {
                    self.nmi_line_low = false; // Trigger NMI (set line low)
                    // println!("PPU: NMI triggered at scanline {}, cycle {}", self.scanline, self.cycle);
                }
            }
        }

        // --- Cycle and Scanline Advancement ---
        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > 261 { // Wrap around after scanline 261 (pre-render scanline)
                self.scanline = -1; // Reset to pre-render scanline for next frame
                self.frame_complete = true;
                self.frame_counter = self.frame_counter.wrapping_add(1);
                self.nmi_line_low = true; // NMI line goes high after VBlank/frame end
                // println!("PPU: Frame {} complete", self.frame_counter);
            }
        }

        // Return NMI line status (active low)
        !self.nmi_line_low
    }

    // ★★★ 背景シフトレジスタを更新するメソッド ★★★
    // Shifts the pattern and attribute registers left by 1
    fn update_background_shifters(&mut self) {
        if self.mask.show_background() {
            self.bg_shifter_pattern_lo <<= 1;
            self.bg_shifter_pattern_hi <<= 1;
            self.bg_shifter_attrib_lo <<= 1;
            self.bg_shifter_attrib_hi <<= 1;
        }
    }

    // ★★★ シフトレジスタにバックグラウンドタイルデータをロードするメソッド ★★★
    // Loads the lower 8 bits of the shifters with the data fetched for the next tile.
    // Should be called after the PT High byte fetch is complete (e.g., cycle 8, 16, ...).
    fn load_background_shifters(&mut self) {
        // --- Add Log BEFORE Loading ---
        // Log less frequently
        if self.cycle > 0 && (self.cycle % 32 == 1) && self.scanline >= 0 && (self.scanline % 16 == 0) {
            // println!(
                // "LoadShifters Pre [Cycle {}, Scanline {}]: \
                 // tile_id={:02X}, tile_attr={:02X}, lsb={:02X}, msb={:02X}, \
                 // PRE_pat_lo={:04X}, PRE_pat_hi={:04X}", // Add PRE shifter values
                // self.cycle, self.scanline,
                // self.bg_next_tile_id, self.bg_next_tile_attr,
                // self.bg_next_tile_lsb, self.bg_next_tile_msb,
                // self.bg_shifter_pattern_lo, self.bg_shifter_pattern_hi // Log current shifter values before load
            // );
        }
        // --- End Log BEFORE Loading ---

        // Load pattern table bits into the lower bytes of the 16-bit pattern shifters
        self.bg_shifter_pattern_lo = (self.bg_shifter_pattern_lo & 0xFF00) | self.bg_next_tile_lsb as u16;
        self.bg_shifter_pattern_hi = (self.bg_shifter_pattern_hi & 0xFF00) | self.bg_next_tile_msb as u16;

        // Load attribute table bits. NES PPU loads the 2 bits for the current tile's quadrant
        // for the *next* 8 pixels. So we repeat the two bits 8 times.
        // bg_next_tile_attr already contains the correct 2 bits for the current tile.
        let attr_bit0 = (self.bg_next_tile_attr & 0b01) * 0xFF; // 0 or 0xFF
        let attr_bit1 = ((self.bg_next_tile_attr & 0b10) >> 1) * 0xFF; // 0 or 0xFF

        self.bg_shifter_attrib_lo = (self.bg_shifter_attrib_lo & 0xFF00) | (attr_bit0 as u16);
        self.bg_shifter_attrib_hi = (self.bg_shifter_attrib_hi & 0xFF00) | (attr_bit1 as u16);


        // --- DEBUG LOG AFTER Loading ---
        // Log less frequently
        if self.cycle > 0 && (self.cycle % 32 == 1) && self.scanline >= 0 && (self.scanline % 16 == 0) { // Re-enable this log
             // println!(
                 // "LoadShifters Post[Cycle {}, Scanline {}]: pat_lo={:04X}, pat_hi={:04X}, attr_lo={:04X}, attr_hi={:04X}",
                 // self.cycle, self.scanline,
                 // self.bg_shifter_pattern_lo, self.bg_shifter_pattern_hi,
                 // self.bg_shifter_attrib_lo, self.bg_shifter_attrib_hi
             // );
        }
        // --- END DEBUG LOG ---
    }

    // PPUサイクルごとのスクロール更新 (水平)
    fn increment_scroll_x(&mut self) {
        // Only increment if rendering is enabled
        if self.mask.show_background() || self.mask.show_sprites() {
            // Increment coarse X. If it wraps from 31 to 0, toggle horizontal nametable bit.
            if self.vram_addr.inc_coarse_x() {
                // self.vram_addr.address ^= 0x0400; // Toggle Nametable X bit (handled in inc_coarse_x)
            }
        }
        // Log vram_addr after potential change
        if self.cycle % 32 == 0 { // Log less frequently
            // println!("[IncScrollX Cycle {}] v: {:04X}", self.cycle, self.vram_addr.get());
        }
    }

    // PPUサイクルごとのスクロール更新 (垂直)
    fn increment_scroll_y(&mut self) {
        // Only increment if rendering is enabled
        if self.mask.show_background() || self.mask.show_sprites() {
            // Increment fine Y. If it wraps from 7 to 0...
            if self.vram_addr.inc_fine_y() {
                // Coarse Y increment logic (handling wrap to next nametable or wrap within attribute table)
                // is handled within AddrRegister::inc_fine_y now.
            }
        }
        // Log vram_addr after potential change - REMOVED from here
        // println!("[IncScrollY Cycle {}] v: {:04X}", self.cycle, self.vram_addr.get()); 
    }

    // tからvへ水平関連ビットをコピー
    fn transfer_address_x(&mut self) {
        if self.mask.show_background() || self.mask.show_sprites() {
            // Copy coarse X and Nametable X bit from temp address (t) to vram address (v)
            self.vram_addr.copy_horizontal_bits(&self.temp_vram_addr);
        }
    }

    // tからvへ垂直関連ビットをコピー (プりレンダースキャンラインで使用)
    fn transfer_address_y(&mut self) {
        if self.mask.show_background() || self.mask.show_sprites() {
            // Copy coarse Y, Fine Y, and Nametable Y bit from temp address (t) to vram address (v)
            self.vram_addr.copy_vertical_bits(&self.temp_vram_addr);
        }
    }

    // Pixel rendering process
    fn render_pixel(&mut self) {
        // Get the current pixel coordinates
        let x = self.cycle - 1;
        let y = self.scanline;

        // Ensure we are within the visible screen area
        if x >= SCREEN_WIDTH || y < 0 || y >= SCREEN_HEIGHT as isize {
            return; // Not a visible pixel
        }

        // Background pixel calculation
        let mut bg_pixel = 0u8; // 2-bit pixel value (0-3)
        let mut bg_palette = 0u8; // 2-bit palette select (0-3)

        // If background rendering is enabled
        if self.mask.show_background() {
            // Determine the bit position to select based on fine_x_scroll
            let bit_select = 0x8000 >> self.fine_x_scroll; // Select bit based on fine_x

            // Select bits from pattern shifters
            let p0_pixel = (self.bg_shifter_pattern_lo & bit_select) > 0;
            let p1_pixel = (self.bg_shifter_pattern_hi & bit_select) > 0;
            bg_pixel = ((p1_pixel as u8) << 1) | (p0_pixel as u8);

            // Select bits from attribute shifters
            let bg_pal0 = (self.bg_shifter_attrib_lo & bit_select) > 0;
            let bg_pal1 = (self.bg_shifter_attrib_hi & bit_select) > 0;
            bg_palette = ((bg_pal1 as u8) << 1) | (bg_pal0 as u8);
        }

        // --- スプライト関連を一時的に無効化 ---
        let fg_pixel = 0;
        let fg_palette = 0;
        // let fg_priority = false; // Sprite priority (Placeholder)
        // --- ここまで ---

        let mut pixel = 0;
        let mut palette = 0;

        // Determine final pixel & palette (スプライトを無視)
        if bg_pixel > 0 {
             pixel = bg_pixel;
             palette = bg_palette;
         } else {
             pixel = 0; // Background is transparent
             palette = 0;
         }

        // Look up the final color index in the palette RAM
        let palette_idx = (palette << 2) | pixel; // Combine palette and pixel index
        let color_idx = self.read_palette_ram(palette_idx as u16);

        // Get the RGB color from the system palette
        let (r, g, b) = NES_PALETTE[(color_idx & 0x3F) as usize]; // Mask with 0x3F to ensure index is within bounds

        // Calculate the index in the frame buffer
        let pixel_index = (self.scanline as usize * self.frame.width + self.cycle - 1) * 4; // RGBA

        // Ensure the index is within bounds before writing RGBA
        if pixel_index + 3 < self.frame.pixels.len() { 
            self.frame.pixels[pixel_index] = r;     // Write R
            self.frame.pixels[pixel_index + 1] = g; // Write G
            self.frame.pixels[pixel_index + 2] = b; // Write B
            self.frame.pixels[pixel_index + 3] = 255; // Write Alpha (Opaque)
        }
    }

    // Generate frame data for test mode
    pub fn get_test_frame(&mut self) -> FrameData {
        // println!("Getting test frame from PPU");
        
        // Make sure we have the debug pattern drawn
        self.draw_debug_pattern();
        
        // Create a new frame data
        let mut frame = FrameData::new(self.frame.width, self.frame.height);
        
        // Copy pixel data from the internal frame buffer
        frame.pixels.copy_from_slice(&self.frame.pixels);
        
        // Count non-zero pixels for debugging
        let non_zero = frame.pixels.iter().filter(|&p| *p != 0).count();
        // println!("Test frame contains {} non-zero pixels out of {}", 
        //         non_zero, frame.pixels.len());
        
        frame
    }

    pub fn render_frame(&mut self) -> FrameData {
        // 現在のフレームを返す
        let frame = FrameData {
            pixels: self.frame.pixels.clone(),
            width: SCREEN_WIDTH,
            height: SCREEN_HEIGHT,
        };
        
        // Count non-black pixels (where R, G, or B is non-zero)
        let non_black_pixels = frame.pixels
            .chunks_exact(4) // Iterate over RGBA chunks
            .filter(|rgba| rgba[0] != 0 || rgba[1] != 0 || rgba[2] != 0) // Check if R, G, or B is non-zero
            .count();

        // println!("PPU render_frame: non-black pixels: {} / total pixels: {}", non_black_pixels, frame.width * frame.height);

        frame
    }

    // PPUレジスタの読み書きメソッド
    pub fn read_status_peek(&self) -> u8 { // New method to just read without side effects
        // Read the status byte, but only top 3 bits are returned to CPU
        // Lower 5 bits contain noise or stale data from last PPU write
        // For simplicity, we can return the full byte for now, or mask it.
        self.status.register
    }

    // Method to handle side effects of reading $2002
    pub fn handle_status_read_side_effects(&mut self) {
        // Reading status register clears the VBlank flag (bit 7)
        self.status.set_vblank_started(false); // Use setter method

        // Reading status register resets the address latch used by $2005/$2006
        self.address_latch_low = true;

        // NMI line goes high immediately after status read if VBlank was set
        // This logic might be better handled in the Bus or Emulator where NMI line state is managed
        self.nmi_line_low = true; 
    }

    pub fn read_oam_data(&self) -> u8 {
        self.oam_data[self.oam_addr as usize]
    }

    pub fn get_vram_address(&self) -> u16 {
        self.vram_addr.addr()
    }

    pub fn handle_data_read_side_effects(&mut self, last_read_value: u8) -> u8 {
        let vram_addr = self.vram_addr.addr();
        let result = if vram_addr >= 0x3F00 {
            // Palette reads are not buffered, return the value read directly,
            // but buffer is filled with *underlying* VRAM data at that address mirror.
            // This detail might need careful checking against hardware tests.
            // For now, return the direct value, update buffer based on VRAM.
            // self.data_buffer = self.read_vram(vram_addr); // Read underlying VRAM/CHR
             last_read_value // Return direct palette data
        } else {
            // Normal VRAM/CHR reads return the *buffered* value
            let buffered_value = self.data_buffer;
            self.data_buffer = last_read_value; // Update buffer with newly read value
            buffered_value
        };
        // Increment address AFTER buffer logic
        self.increment_vram_addr();
        result
    }

    pub fn increment_vram_addr(&mut self) {
        // println!("[PPU] increment_vram_addr called. Current addr: ${:04X}", self.vram_addr.addr()); // ★★★ Log entry
        let increment = if self.ctrl.vram_addr_increment() != 0 { 32 } else { 1 }; // Check if the flag is non-zero
        self.vram_addr.increment(increment);
        // VRAM addresses wrap around above $3FFF, actual mirroring handled by bus read/write
        // self.vram_addr.set(self.vram_addr.get() % 0x4000); // Don't do simplified wrapping here
        // println!("[PPU Inc VRAM Addr] Incremented by {}. New Addr Reg: {:?}", increment, self.vram_addr);
    }

    pub fn write_ctrl(&mut self, data: u8) {
        let old_nmi_enable = self.ctrl.generate_nmi(); // Use helper method
        let old_bits = self.ctrl.bits();
        // println!("[PPU Write CTRL] Old bits=${:02X}, New bits=${:02X}", old_bits, data);
        // println!("[PPU Write CTRL] Old BG Pattern=${:04X}, New BG Pattern will be=${:04X}",
                 // if (old_bits & 0x10) == 0 { 0x0000 } else { 0x1000 },
                 // if (data & 0x10) == 0 { 0x0000 } else { 0x1000 });
        
        self.ctrl.set_bits(data); // Use setter method
        
        // Verify the bits were actually set
        // println!("[PPU Write CTRL] Verification: CTRL bits=${:02X}, BG Pattern=${:04X}, NMI={}, Sprite Pattern=${:04X}, VRAM Inc={}",
                 // self.ctrl.bits(), self.ctrl.background_pattern_addr(),
                 // self.ctrl.generate_nmi(), self.ctrl.sprite_pattern_addr(),
                 // self.ctrl.vram_addr_increment());
        
        // Update temp VRAM address nametable select bits
        let nametable_select = data & 0b11;
        self.temp_vram_addr.set_nametable_select(nametable_select);

        // Trigger NMI if VBlank flag is set AND NMI was just enabled
        let nmi_now_enabled = self.ctrl.generate_nmi(); // Use helper method
        if self.status.vblank_started() && !old_nmi_enable && nmi_now_enabled { // Use helper method
            // Trigger NMI immediately
            self.nmi_line_low = false;
        }
    }

    pub fn write_mask(&mut self, data: u8) {
        self.mask.set_bits(data); // Use setter method
    }

    pub fn write_oam_addr(&mut self, data: u8) {
        self.oam_addr = data;
    }

    pub fn write_oam_data(&mut self, data: u8) {
        self.oam_data[self.oam_addr as usize] = data;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    pub fn write_scroll(&mut self, data: u8) {
        // <<< Add Log >>>
        // println!("[PPU $2005 Write] Data=${:02X}, Latch={}", data, self.address_latch_low);

        if self.address_latch_low {
            // First write (X scroll)
            self.temp_vram_addr.set_coarse_x(data >> 3);
            self.fine_x_scroll = data & 0x07;
            // <<< Add Log >>>
            // println!("  -> First write: coarse_x={}, fine_x={}", data >> 3, self.fine_x_scroll);
            self.address_latch_low = false;
        } else {
            // Second write (Y scroll)
            self.temp_vram_addr.set_fine_y(data & 0x07);
            self.temp_vram_addr.set_coarse_y(data >> 3);
             // <<< Add Log >>>
            // println!("  -> Second write: coarse_y={}, fine_y={}", data >> 3, data & 0x07);
           self.address_latch_low = true;
        }
    }

    pub fn write_addr(&mut self, data: u8) {
        if self.address_latch_low { // First write (High byte)
            self.temp_vram_addr.set_high_byte(data & 0x3F);
            self.address_latch_low = false;
        } else { // Second write (Low byte)
            self.temp_vram_addr.set_low_byte(data);
            // Copy t to v immediately on second write
            self.vram_addr.copy_from(&self.temp_vram_addr);
            self.address_latch_low = true;
            // Log the transfer (optional)
            // println!("[PPU Addr Write] Transferred t({:04X}) to v({:04X})", self.temp_vram_addr.get(), self.vram_addr.get());
        }
    }

    pub fn write_palette(&mut self, addr: u8, data: u8) {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F,
            _ => index,
        };
        self.palette_ram[mirrored_index] = data & 0x3F;
    }

    pub fn write_oam_byte(&mut self, index: u8, data: u8) {
        self.oam_data[index as usize] = data;
    }

    // デバッグパターンを描画するメソッド
    pub fn draw_debug_pattern(&mut self) {
        // フレームバッファをクリア
        for i in 0..self.frame.pixels.len() {
            self.frame.pixels[i] = 0;
        }

        // フレームの寸法
        let width = self.frame.width;
        let height = self.frame.height;

        // カラーバーを描画
        let colors = [
            (255, 0, 0),    // 赤
            (0, 255, 0),    // 緑
            (0, 0, 255),    // 青
            (255, 255, 0),  // 黄
            (0, 255, 255),  // シアン
            (255, 0, 255),  // マゼンタ
            (255, 255, 255) // 白
        ];

        let bar_width = width / colors.len();

        // カラーバーを描画
        for (i, &(r, g, b)) in colors.iter().enumerate() {
            let x_start = i * bar_width;
            let x_end = (i + 1) * bar_width;

            for y in 0..height {
                for x in x_start..x_end {
                    let pixel_index = (y * width + x) * 4;
                    if pixel_index + 3 < self.frame.pixels.len() {
                        self.frame.pixels[pixel_index] = r;
                        self.frame.pixels[pixel_index + 1] = g;
                        self.frame.pixels[pixel_index + 2] = b;
                        self.frame.pixels[pixel_index + 3] = 255;
                    }
                }
            }
        }
    }

    // VRAMアドレスのミラーリングを行う
    pub fn mirror_vram_addr(&self, addr: u16, mirroring: Mirroring) -> usize {
        // Ensure address is within PPU VRAM range ($2000-$3FFF)
        let addr_masked = addr & 0x3FFF;

        if addr_masked >= 0x2000 && addr_masked <= 0x3EFF {
            let relative_addr = addr_masked & 0x0FFF; // Address relative to $2000 (0x0000 - 0x0FFF)
            let table = relative_addr / 0x0400;    // Nametable index (0, 1, 2, 3)
            let offset = relative_addr & 0x03FF;   // Offset within the nametable (0x000 - 0x3FF)

            match mirroring {
                Mirroring::Horizontal => {
                    // Map tables 0,1 to physical 0; tables 2,3 to physical 1 (0x400 offset)
                    let physical_table = (table >> 1) & 1; // 0 or 1
                    ((physical_table * 0x400) + offset) as usize
                }
                Mirroring::Vertical => {
                    // Map tables 0,2 to physical 0; tables 1,3 to physical 1 (0x400 offset)
                    let physical_table = table & 1; // 0 or 1
                    ((physical_table * 0x400) + offset) as usize
                }
                Mirroring::SingleScreenLower => {
                    // Map all to physical 0
                    offset as usize
                }
                Mirroring::SingleScreenUpper => {
                    // Map all to physical 1 (0x400 offset)
                    (0x400 + offset) as usize
                }
                Mirroring::FourScreen => {
                    // No mirroring, use relative address directly (potentially requires extra RAM)
                    relative_addr as usize
                }
            }
        } else {
            // Address is outside the Nametable/Attribute table range ($2000-$3EFF)
            // This function is primarily for VRAM mirroring.
            // CHR reads ($0000-$1FFF) or Palette reads ($3F00-$3FFF) shouldn't rely on this.
            eprintln!("[WARN] mirror_vram_addr called with non-VRAM address: {:04X}", addr);
            // Return a masked address, but this indicates a potential logic error elsewhere
            (addr_masked & 0x07FF) as usize // Return address within 2KB range as a fallback
        }
    }

    // VRAMからデータを読み取る (BusAccess経由ではなく内部VRAM/CHR RAM用)
    pub fn read_vram_internal(&self, addr: u16) -> u8 {
        let addr_masked = addr & 0x3FFF; // 14ビットアドレス空間に制限

        if addr_masked <= 0x1FFF {
            // パターンテーブル ($0000-$1FFF) - CHR RAMから読み取り
            return self.chr_ram[addr_masked as usize];
        } else if addr_masked <= 0x3EFF {
            // ネームテーブル ($2000-$2FFF), ミラー ($3000-$3EFF)
            let mirrored_addr = self.mirror_vram_addr(addr_masked, self.mirroring);
            return self.vram[mirrored_addr];
        } else {
            // パレットデータ ($3F00-$3FFF)
            // Use self directly
            return self.read_palette_ram(addr_masked);
        }
    }

    // Direct read from palette RAM, handling mirroring
    fn read_palette_ram(&self, addr: u16) -> u8 {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // $3F1x → $3F0x mirror
            _ => index,
        };
        self.palette_ram[mirrored_index]
    }

    // Write to palette RAM, handling mirroring
    fn write_palette_ram(&mut self, addr: u16, data: u8) {
        let mapped_addr = (addr & 0x001F) as usize; // Ensure address is within 0-31
        // Handle palette mirroring ($3F10/$3F14/$3F18/$3F1C mirror $3F00/$3F04/$3F08/$3F0C)
        let final_addr = match mapped_addr {
            0x10 => 0x00,
            0x14 => 0x04,
            0x18 => 0x08,
            0x1C => 0x0C,
            _ => mapped_addr,
        };
        // println!("[PPU Write Palette] Writing Data=${:02X} to Addr=${:04X} (Mapped from {:04X})", data, final_addr, addr);
        self.palette_ram[final_addr] = data;
    }

    // Reads data from the specified VRAM address (handling palettes)
    // but *without* the read buffer delay or address increment side effects.
    // This uses the BUS to access underlying memory (CHR or VRAM via mirroring).
    pub fn read_data_peek(&self, bus: &impl BusAccess, addr: u16) -> u8 {
        let addr_masked = addr & 0x3FFF;
        if addr_masked >= 0x3F00 {
            // Direct read from palette RAM (no buffering for palettes)
            // Use self directly
             self.read_palette_ram(addr_masked)
        } else {
            // Read from VRAM or CHR via bus (use ppu_read_vram helper)
             bus.ppu_read_vram(addr_masked) // Assuming bus has ppu_read_vram
        }
    }

    // $2007 PPUDATA Write Handler
    pub fn write_data(&mut self, data: u8, bus: &mut impl BusAccess) {
        let addr = self.vram_addr.get();
        // println!("[PPU $2007 Write] write_data function entered! VRAM Addr=${:04X}, Data=${:02X}", addr, data); // ★★★ Log entry

        // Write to appropriate memory (Palette RAM or VRAM/CHR via bus)
        if addr >= 0x3F00 {
            // Palette RAM write
            // println!("[PPU $2007 Write] Writing to Palette RAM addr=${:04X}", addr);
            self.write_palette_ram(addr, data); // Use the existing internal function
        } else {
            // VRAM/CHR write via bus
            // println!("[PPU $2007 Write] Writing to VRAM/CHR via bus addr=${:04X}", addr);
            bus.ppu_write_vram(addr, data); // <- 修正後: PPU VRAM 書き込み用のメソッドを使用
        }

        // Increment VRAM address based on PPUCTRL setting
        self.increment_vram_addr();
        // println!("[PPU $2007 Write] VRAM address incremented. New VRAM Addr=${:04X}", self.vram_addr.get());
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

// --- FrameData Definition (Re-added at the end) ---
#[derive(Debug, Clone, Serialize)]
pub struct FrameData {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl FrameData {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![0; width * height * 4], // RGBA
            width,
            height,
        }
    }
}

impl Default for FrameData {
    fn default() -> Self {
        Self::new(SCREEN_WIDTH, SCREEN_HEIGHT)
    }
}
