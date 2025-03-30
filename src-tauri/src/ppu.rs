use serde::Serialize; // Import Serialize
use crate::Mirroring; // Ensure Mirroring is imported from crate root (main.rs)
use crate::bus::Bus;             // Ensure Bus is imported
use crate::registers::{AddrRegister, ControlRegister, MaskRegister, StatusRegister}; // Assuming registers module exists
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

pub struct Ppu {
    // PPUレジスタ (Busからアクセスするため pub に変更)
    pub ctrl: ControlRegister,
    pub mask: MaskRegister,
    pub status: StatusRegister,
    pub oam_addr: u8,
    // oam_data: u8, // $2004 Read/Write (Direct OAM access often handled differently)

    // Internal state (Busからアクセスするため pub に変更)
    pub cycle: u16,          // Cycle count for the current scanline
    pub scanline: u16,       // Current scanline number (0-261)
    pub frame: FrameData,    // Framebuffer for the current frame
    pub nmi_line_low: bool,  // Flag indicating NMI should be triggered
    // pub nmi_output: bool,    // Whether NMI generation is enabled (from CTRL register)

    // VRAM / Address Registers (Busからアクセスするため pub に変更)
    pub vram_addr: AddrRegister, // Use the VRamRegister struct
    pub temp_vram_addr: AddrRegister, // Temporary VRAM address ('t')
    pub address_latch_low: bool, // For $2005/$2006 writes
    pub fine_x_scroll: u8, // Fine X scroll (3 bits)

    // Data I/O (Busからアクセスするため pub に変更)
    pub data_buffer: u8,   // Buffer for $2007 reads

    // OAM (Object Attribute Memory) (Busからアクセスするため pub に変更)
    pub oam_data: [u8; 256],

    // Palette RAM (32 bytes)
    pub palette_ram: [u8; 32],

    pub vram: Vec<u8>,         // Nametable RAM (2KB)
    pub frame_complete: bool,
    pub chr_ram: Vec<u8>, // CHR RAM for testing
    pub write_latch: bool,           // Loopy's w register

    // Internal rendering state (might be needed for accurate timing)
    bg_next_tile_id: u8,
    bg_next_tile_attrib: u8,
    bg_next_tile_lsb: u8,
    bg_next_tile_msb: u8,
    bg_shifter_pattern_lo: u16,
    bg_shifter_pattern_hi: u16,
    bg_shifter_attrib_lo: u16,
    bg_shifter_attrib_hi: u16,
    // アニメーション用フレームカウンタを追加
    pub frame_counter: u32,
}

impl Ppu {
    pub fn new() -> Self {
        let mut ppu = Self {
            ctrl: ControlRegister::new(),
            mask: MaskRegister::new(),
            status: StatusRegister::new(),
            oam_addr: 0x00,
            cycle: 0, // Start at cycle 0
            scanline: 261, // Start at pre-render scanline for proper init
            frame: FrameData::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            nmi_line_low: true,
            vram_addr: AddrRegister::new(),
            temp_vram_addr: AddrRegister::new(),
            address_latch_low: true, // Initial state of $2005/$2006 latch
            fine_x_scroll: 0,
            data_buffer: 0,
            oam_data: [0; 256],
            palette_ram: [0; 32],
            vram: vec![0; 2048],
            frame_complete: false,
            chr_ram: vec![0; 8192], // Remove if using Cartridge CHR directly
            write_latch: false,

            // Initialize internal rendering state
            bg_next_tile_id: 0,
            bg_next_tile_attrib: 0,
            bg_next_tile_lsb: 0,
            bg_next_tile_msb: 0,
            bg_shifter_pattern_lo: 0,
            bg_shifter_pattern_hi: 0,
            bg_shifter_attrib_lo: 0,
            bg_shifter_attrib_hi: 0,
            frame_counter: 0,
        };
        
        // パレットの初期化 - NESの標準パレットに近い色を設定
        // 背景色（グレー）
        ppu.palette_ram[0] = 0x0F;
        
        // パレット0（青、赤、緑の組み合わせ）
        ppu.palette_ram[1] = 0x01; // 青色
        ppu.palette_ram[2] = 0x21; // 赤色
        ppu.palette_ram[3] = 0x31; // 緑色
        
        // パレット1（紫、黄色、水色の組み合わせ）
        ppu.palette_ram[5] = 0x14; // 紫色
        ppu.palette_ram[6] = 0x27; // 黄色
        ppu.palette_ram[7] = 0x2A; // 水色
        
        // パレット2（黄緑、赤紫、オレンジの組み合わせ）
        ppu.palette_ram[9] = 0x1A; // 黄緑
        ppu.palette_ram[10] = 0x13; // 赤紫
        ppu.palette_ram[11] = 0x26; // オレンジ
        
        // パレット3（白、暗い青、暗い緑の組み合わせ）
        ppu.palette_ram[13] = 0x30; // 白
        ppu.palette_ram[14] = 0x02; // 暗い青
        ppu.palette_ram[15] = 0x19; // 暗い緑
        
        // ミラーリングのために $3F10、$3F14、$3F18、$3F1Cは$3F00、$3F04、$3F08、$3F0Cと同じ
        ppu.palette_ram[0x10] = ppu.palette_ram[0];
        ppu.palette_ram[0x14] = ppu.palette_ram[0x04];
        ppu.palette_ram[0x18] = ppu.palette_ram[0x08];
        ppu.palette_ram[0x1C] = ppu.palette_ram[0x0C];
        
        // 初期テストパターンを描画
        ppu.init_test_pattern();
        
        // 初期状態のフラグを設定（画面を表示するための準備）
        // PPUコントロールレジスタは標準的なNMI設定に
        ppu.ctrl.set_bits(0x90); // Generate NMI at VBlank, use 8x8 sprites, use first pattern table
        
        // PPUマスクレジスタはまだ表示しない設定
        ppu.mask.set_bits(0x00); // 表示はエミュレータループの中で有効にする
        
        ppu
    }

    // テスト用のパターンを描画
    fn init_test_pattern(&mut self) {
        // シンプルなパターンを作成
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let index = (y * SCREEN_WIDTH + x) * 4;
                
                // 画面を4つの領域に分ける
                let color = if x < SCREEN_WIDTH/2 && y < SCREEN_HEIGHT/2 {
                    // 左上: 赤
                    (255, 0, 0)
                } else if x >= SCREEN_WIDTH/2 && y < SCREEN_HEIGHT/2 {
                    // 右上: 緑
                    (0, 255, 0)
                } else if x < SCREEN_WIDTH/2 && y >= SCREEN_HEIGHT/2 {
                    // 左下: 青
                    (0, 0, 255)
                } else {
                    // 右下: 黄色
                    (255, 255, 0)
                };
                
                self.frame.pixels[index] = color.0;
                self.frame.pixels[index + 1] = color.1;
                self.frame.pixels[index + 2] = color.2;
                self.frame.pixels[index + 3] = 255; // Alpha
            }
        }
    }

    // PPUをリセットするメソッド
    pub fn reset(&mut self) {
        // PPUの内部状態をリセット
        self.cycle = 0;
        self.scanline = 0;
        self.frame_complete = false;
        self.nmi_line_low = true;
        self.address_latch_low = true;
        self.fine_x_scroll = 0;
        self.data_buffer = 0;
        self.oam_addr = 0;
        self.frame_counter = 0;

        // PPUレジスタをリセット
        self.status.register = 0x00;
        self.mask.set_bits(0x00);  // 表示無効化
        self.ctrl.set_bits(0x00);  // NMI無効化

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
        
        // 初期状態のフラグを設定
        self.ctrl.set_bits(0x90);  // NMI有効化など
        self.mask.set_bits(0x1E);  // 背景とスプライトを有効化
        
        println!("PPU Reset complete - Blue color scheme initialized");
    }

    // Simulate PPU stepping by ONE PPU cycle
    pub fn step(&mut self) { // Correct signature: No arguments
        // --- Logic based on current cycle/scanline ---
        let current_scanline = self.scanline;
        let current_cycle = self.cycle;

        // --- VBlank/NMI Timing --- (Actions happen based on the state *at* this cycle)
        if current_scanline == 241 && current_cycle == 1 {
            self.status.set_vblank_started(true);
            self.frame_complete = true; // Signal frame ready
            
            // フレームが完了したらフレームカウンタをインクリメント
            self.frame_counter = self.frame_counter.wrapping_add(1);
            
            // 100フレームごとにデバッグ出力
            if self.frame_counter % 100 == 0 {
                println!("PPU Frame: {}", self.frame_counter);
            }
        }
        if current_scanline == 261 && current_cycle == 1 { // Pre-render line start
            self.status.set_vblank_started(false);
            self.status.set_sprite_overflow(false);
            self.status.set_sprite_zero_hit(false);
        }

        // --- Rendering Logic --- 
        // ビジブルラインの場合のみ、基本的なピクセル計算を行う
        let rendering_enabled = self.mask.show_background() || self.mask.show_sprites();
        let is_visible_scanline = current_scanline < 240;
        
        if is_visible_scanline && current_cycle >= 1 && current_cycle <= 256 {
            let x = (current_cycle - 1) as usize;
            let y = current_scanline as usize;
            
            if rendering_enabled {
                // --- リッチアニメーションシステム ---
                
                // ベースとなる時間ファクター
                let main_time = self.frame_counter as f32 * 0.02;
                let slow_time = self.frame_counter as f32 * 0.01;
                let fast_time = self.frame_counter as f32 * 0.04;
                
                // 画面位置の正規化座標 (0.0 〜 1.0)
                let nx = x as f32 / SCREEN_WIDTH as f32;
                let ny = y as f32 / SCREEN_HEIGHT as f32;
                
                // 画面中心からの距離
                let cx = nx - 0.5;
                let cy = ny - 0.5;
                let dist_center = (cx * cx + cy * cy).sqrt();
                
                // 複数のアニメーションパターン生成
                
                // 1. モザイクパターン (大きさが時間とともに変化)
                let mosaic_size = 4.0 + (main_time.sin() * 0.5 + 0.5) * 20.0;
                let mosaic_x = (x as f32 / mosaic_size).floor() as i32;
                let mosaic_y = (y as f32 / mosaic_size).floor() as i32;
                let mosaic = (mosaic_x + mosaic_y) % 2 == 0;
                
                // 2. 同心円パターン (中心から広がる波)
                let circle_speed = 1.0 + (slow_time.cos() * 0.5 + 0.5) * 3.0;
                let circle_phase = (dist_center * 10.0 - main_time * circle_speed) % 1.0;
                let circle = circle_phase > 0.5;
                
                // 3. 螺旋パターン
                let angle = ny.atan2(nx) * 3.0;
                let spiral = ((angle + main_time) % 1.0) > 0.5;
                
                // 4. 交差する波パターン
                let wave_x = (nx * 10.0 + main_time).sin();
                let wave_y = (ny * 10.0 + main_time * 0.7).cos();
                let waves = (wave_x * wave_y) > 0.0;
                
                // 5. 複合パターン - 時間によって変化
                let time_segment = ((self.frame_counter / 180) % 4) as usize;
                let pattern = match time_segment {
                    0 => mosaic as usize,
                    1 => circle as usize,
                    2 => spiral as usize,
                    _ => waves as usize,
                };
                
                // 色パレットセット（時間で循環）
                let color_sets = [
                    // 青と水色のセット
                    [0x01, 0x11, 0x21, 0x31, 0x0C, 0x1C, 0x2C, 0x3C],
                    // 緑と黄緑のセット
                    [0x09, 0x19, 0x29, 0x39, 0x0A, 0x1A, 0x2A, 0x3A],
                    // 紫とピンクのセット
                    [0x04, 0x14, 0x24, 0x34, 0x05, 0x15, 0x25, 0x35],
                    // 赤とオレンジのセット
                    [0x06, 0x16, 0x26, 0x36, 0x07, 0x17, 0x27, 0x37],
                ];
                
                // カラーセットの選択（90秒ごとに変更）
                let color_set_index = ((self.frame_counter / (60 * 90)) % color_sets.len() as u32) as usize;
                let colors = &color_sets[color_set_index];
                
                // 明暗の選択と色相シフトを時間で変化
                let hue_shift = ((main_time * 0.5).sin() * 0.5 + 0.5) * 4.0;
                let shade_index = (hue_shift as usize) % 4;
                
                // パターンに基づく色の選択
                let color_index = match pattern {
                    1 => colors[shade_index],          // 明るい色
                    _ => colors[4 + (shade_index % 4)], // 暗い色
                };
                
                // 特殊効果 - フラッシュと暗転
                let special_effect = match self.frame_counter % 600 {
                    597..=599 => true, // 暗転効果
                    298..=300 => true, // 中間フラッシュ
                    _ => false,
                };
                
                // 時々画面の一部に波紋エフェクト
                let ripple_effect = self.frame_counter % 300 >= 150 && self.frame_counter % 300 < 180;
                let ripple_intensity = if ripple_effect {
                    let t = (self.frame_counter % 300 - 150) as f32 / 30.0;
                    let phase = t * std::f32::consts::PI * 2.0;
                    let ripple_distance = (dist_center * 20.0 - fast_time * 5.0).sin() * 0.5 + 0.5;
                    (phase.sin() * ripple_distance) * 0.7 + 0.3
                } else {
                    1.0
                };
                
                // 最終的な色の決定
                let final_color = if special_effect {
                    0x0F // 黒色
                } else if ripple_effect && ripple_intensity < 0.5 {
                    // 波紋の暗い部分
                    0x0F
                } else {
                    color_index
                };
                
                // NESパレットからRGB値を取得
                let (r, g, b) = NES_PALETTE[final_color as usize & 0x3F];
                
                // フレームバッファに書き込み
                let frame_idx = (y * SCREEN_WIDTH + x) * 4;
                if frame_idx + 3 < self.frame.pixels.len() {
                    self.frame.pixels[frame_idx] = r;
                    self.frame.pixels[frame_idx + 1] = g;
                    self.frame.pixels[frame_idx + 2] = b;
                    self.frame.pixels[frame_idx + 3] = 255;
                }
            }
        }

        // --- PPU timing simulation ---
        self.cycle += 1;
        if self.cycle > 340 { // End of scanline
            self.cycle = 0;
            self.scanline += 1;
            
            if self.scanline > 261 { // End of frame
                self.scanline = 0;
            }
        }
        
        // --- NMI Line Update --- 
        let nmi_asserted = self.status.vblank_started() && self.ctrl.generate_nmi();
        self.nmi_line_low = !nmi_asserted; // Line is low (active) when asserted
    }

    // Placeholder for PPU reads - Replace with Bus calls eventually
    fn ppu_read_placeholder(&self, addr: u16) -> u8 {
        match addr & 0x3FFF {
            0x0000..=0x1FFF => self.chr_ram.get(addr as usize).copied().unwrap_or(0), // Use CHR RAM
            0x2000..=0x3EFF => { // Simulate VRAM read with mirroring
                let mirrored_addr = self.mirror_vram_addr(addr, Mirroring::Vertical); // Placeholder mirroring
                 self.vram.get(mirrored_addr as usize).copied().unwrap_or(0)
            }
            0x3F00..=0x3FFF => self.read_palette_ram(addr), // Read directly from palette RAM
            _ => 0,
        }
    }

    // Direct read from palette RAM, handling mirroring
    fn read_palette_ram(&self, addr: u16) -> u8 {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // Mirror $3F1x to $3F0x
            _ => index,
        };
        self.palette_ram[mirrored_index] & 0x3F // Mask to 6 bits
    }

    // Helper to get pixel from background shifters
    fn get_background_pixel(&self) -> u8 {
        if !self.mask.show_background() { return 0; }
        let bit_select = 0x8000 >> self.fine_x_scroll;
        let p0 = ((self.bg_shifter_pattern_lo & bit_select) > 0) as u8;
        let p1 = ((self.bg_shifter_pattern_hi & bit_select) > 0) as u8;
        let pixel = (p1 << 1) | p0;
        if pixel == 0 { return 0; } // If color is 0, palette high bits don't matter
        let a0 = ((self.bg_shifter_attrib_lo & bit_select) > 0) as u8;
        let a1 = ((self.bg_shifter_attrib_hi & bit_select) > 0) as u8;
        let attrib = (a1 << 1) | a0;
        (attrib << 2) | pixel
    }

    // --- Register Access Methods (Called by Bus) ---
    pub fn read_status(&mut self) -> u8 {
        let status = (self.status.register & 0xE0) | (self.data_buffer & 0x1F);
        self.status.set_vblank_started(false);
        self.address_latch_low = true; // Reset write latch
        status
    }

    // Read data - reverts to taking &Bus to handle complex buffer/read logic
    pub fn read_data(&mut self, bus: &Bus) -> u8 { 
        let addr = self.vram_addr.addr() & 0x3FFF;
        let increment = self.ctrl.vram_addr_increment();
        
        let result = if addr >= 0x3F00 { // Palette read
            let palette_data = bus.read_palette(addr); 
            self.data_buffer = bus.ppu_read_vram(addr - 0x1000); // Buffer VRAM beneath palette mirror
            palette_data
        } else { // VRAM read
            let buffered_data = self.data_buffer; // Return previous buffer contents
            self.data_buffer = bus.ppu_read_vram(addr); // Update buffer with data at current address
            buffered_data
        };
        
        self.vram_addr.increment(increment);
        result
    }

    // Write data - reverts to taking &mut Bus
    pub fn write_data(&mut self, bus: &mut Bus, data: u8) {
        let addr = self.vram_addr.addr() & 0x3FFF;
        let increment = self.ctrl.vram_addr_increment();
        bus.ppu_write_vram(addr, data); // Perform write via Bus helper
        self.vram_addr.increment(increment);
    }

    pub fn write_ctrl(&mut self, data: u8) {
        let old_nmi_enable = self.ctrl.generate_nmi();
        self.ctrl.set_bits(data);
        self.temp_vram_addr.set_nametable_select(data & 0x03);
        let nmi_now_enabled = self.ctrl.generate_nmi();
        if !old_nmi_enable && nmi_now_enabled && self.status.vblank_started() {
            // NMI edge case handled by step/bus clock logic
        }
    }

    pub fn write_mask(&mut self, data: u8) { self.mask.set_bits(data); }
    pub fn write_oam_addr(&mut self, data: u8) { self.oam_addr = data; }
    pub fn write_oam_data(&mut self, data: u8) {
        // TODO: OAM writes ignored during rendering?
        self.oam_data[self.oam_addr as usize] = data;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    pub fn write_scroll(&mut self, data: u8) {
        if self.address_latch_low { // First write (X)
            self.temp_vram_addr.set_coarse_x(data >> 3);
            self.fine_x_scroll = data & 0x07;
            self.address_latch_low = false;
        } else { // Second write (Y)
            self.temp_vram_addr.set_fine_y(data & 0x07);
            self.temp_vram_addr.set_coarse_y(data >> 3);
            self.address_latch_low = true;
        }
    }

    pub fn write_addr(&mut self, data: u8) {
        if self.address_latch_low { // First write (High byte)
            self.temp_vram_addr.set_high_byte(data & 0x3F); // PPU addresses 14-bit
            self.address_latch_low = false;
        } else { // Second write (Low byte)
            self.temp_vram_addr.set_low_byte(data);
            self.vram_addr.copy_from(&self.temp_vram_addr); // Copy t to v
            self.address_latch_low = true;
        }
    }

    // --- Loopy VRAM Helpers --- (Need full implementation based on nesdev wiki)
     fn increment_scroll_x(&mut self) { /* ... */ }
     fn increment_scroll_y(&mut self) { /* ... */ }
     fn transfer_address_x(&mut self) { /* ... */ }
     fn transfer_address_y(&mut self) { /* ... */ }
     fn update_shifters(&mut self) { /* ... */ }
     fn load_background_shifters(&mut self) { /* ... */ }
     pub fn read_oam_data(&self) -> u8 { self.oam_data[self.oam_addr as usize] }
     pub fn mirror_vram_addr(&self, _addr: u16, _mode: Mirroring) -> u16 { 
        // Simplified placeholder - actual logic depends on mirroring mode
        // For example, for Horizontal mirroring:
        // let mirrored_addr = addr & 0b1011_1111_1111; // Clear bit 10
        // For Vertical mirroring:
        // let mirrored_addr = addr & 0b1111_0111_1111; // Clear bit 11
        // return mirrored_addr;
        _addr // Return unmodified address for now
     }
     pub fn get_frame(&self) -> FrameData { self.frame.clone() } // Use FrameData below

    // VRAM Access Helpers (バスからのアクセス用)
    pub fn get_vram_address(&self) -> u16 {
        self.vram_addr.addr() // AddrRegisterのaddr()メソッドを使用
    }

    pub fn increment_vram_addr(&mut self) {
        // ControlRegisterのvram_addr_increment()はu16型の値を返すので、直接使用できる
        let increment = self.ctrl.vram_addr_increment();
        self.vram_addr.increment(increment);
    }

    pub fn handle_data_read_buffer(&mut self, new_data: u8) -> u8 {
        let result = if (self.vram_addr.addr() & 0x3FFF) >= 0x3F00 {
            // パレットデータの読み出しの場合はバッファを経由せず直接返す
            new_data
        } else {
            // それ以外のVRAM領域の場合は前回のバッファの内容を返し、バッファを更新
            let old_buffer = self.data_buffer;
            self.data_buffer = new_data;
            old_buffer
        };
        result
    }

    // OAM DMA処理
    pub fn write_oam_dma(&mut self, page: u8) {
        // この実装はダミー（実際のDMA処理はBus側で行われる）
        println!("PPU OAM DMA triggered with page ${:02X}", page);
    }

    // VRAM読み書き
    pub fn read_vram(&self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF; // 14ビットアドレスマスク
        match addr {
            0x0000..=0x1FFF => {
                // パターンテーブル (CHR ROM/RAM)
                // 実際にはこの部分は外部（Cartridge）によって処理される
                self.chr_ram.get(addr as usize).copied().unwrap_or(0)
            }
            0x2000..=0x3EFF => {
                // ネームテーブル（PPU内部VRAM）
                let mirrored_addr = self.mirror_vram_addr(addr, Mirroring::Vertical) & 0x07FF;
                self.vram.get(mirrored_addr as usize).copied().unwrap_or(0)
            }
            _ => 0 // 不正なアドレス
        }
    }

    pub fn write_vram(&mut self, addr: u16, data: u8) {
        let addr = addr & 0x3FFF; // 14ビットアドレスマスク
        match addr {
            0x0000..=0x1FFF => {
                // パターンテーブル (CHR ROM/RAM)
                // 実際にはこの部分は外部（Cartridge）によって処理される
                if (addr as usize) < self.chr_ram.len() {
                    self.chr_ram[addr as usize] = data;
                }
            }
            0x2000..=0x3EFF => {
                // ネームテーブル（PPU内部VRAM）
                let mirrored_addr = self.mirror_vram_addr(addr, Mirroring::Vertical) & 0x07FF;
                if (mirrored_addr as usize) < self.vram.len() {
                    self.vram[mirrored_addr as usize] = data;
                }
            }
            _ => {} // 不正なアドレス
        }
    }

    // パレットRAM用メソッド
    pub fn read_palette(&self, addr: u8) -> u8 {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // $3F1x → $3F0x ミラー
            _ => index,
        };
        self.palette_ram[mirrored_index]
    }

    pub fn write_palette(&mut self, addr: u8, data: u8) {
        let index = (addr & 0x1F) as usize;
        let mirrored_index = match index {
            0x10 | 0x14 | 0x18 | 0x1C => index & 0x0F, // $3F1x → $3F0x ミラー
            _ => index,
        };
        self.palette_ram[mirrored_index] = data & 0x3F; // 6ビットにマスク
    }
}

impl Default for Ppu { fn default() -> Self { Self::new() } }

// --- FrameData Definition (Re-added at the end) ---
#[derive(Clone, Serialize)]
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
