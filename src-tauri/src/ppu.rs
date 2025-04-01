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
            vram: [0; 2048],
            frame_complete: false,
            chr_ram: [0; 8192], // Remove if using Cartridge CHR directly

            // Initialize internal rendering state
            bg_next_tile_id: 0,
            bg_next_tile_attr: 0,
            bg_next_tile_lsb: 0,
            bg_next_tile_msb: 0,
            bg_shifter_pattern_lo: 0,
            bg_shifter_pattern_hi: 0,
            bg_shifter_attrib_lo: 0,
            bg_shifter_attrib_hi: 0,
            frame_counter: 0,

            mirroring: Mirroring::FourScreen,
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
        println!("初期テストパターンを描画中...");
        
        // フレームバッファをクリア
        for i in 0..self.frame.pixels.len() {
            self.frame.pixels[i] = 0;
        }
        
        // グラデーションパターンを作成
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let index = (y * SCREEN_WIDTH + x) * 4;
                
                // グラデーションカラーの計算
                let r = (x * 255 / SCREEN_WIDTH) as u8;
                let g = (y * 255 / SCREEN_HEIGHT) as u8;
                let b = ((x + y) * 128 / (SCREEN_WIDTH + SCREEN_HEIGHT)) as u8;
                
                // フレームバッファに色を設定
                self.frame.pixels[index] = r;
                self.frame.pixels[index + 1] = g;
                self.frame.pixels[index + 2] = b;
                self.frame.pixels[index + 3] = 255; // Alpha
            }
        }
        
        // 白い十字線を描画
        let center_x = SCREEN_WIDTH / 2;
        let center_y = SCREEN_HEIGHT / 2;
        
        for i in 0..SCREEN_WIDTH {
            let h_index = (center_y * SCREEN_WIDTH + i) * 4;
            if h_index + 3 < self.frame.pixels.len() {
                self.frame.pixels[h_index] = 255;
                self.frame.pixels[h_index + 1] = 255;
                self.frame.pixels[h_index + 2] = 255;
                self.frame.pixels[h_index + 3] = 255;
            }
        }
        
        for i in 0..SCREEN_HEIGHT {
            let v_index = (i * SCREEN_WIDTH + center_x) * 4;
            if v_index + 3 < self.frame.pixels.len() {
                self.frame.pixels[v_index] = 255;
                self.frame.pixels[v_index + 1] = 255;
                self.frame.pixels[v_index + 2] = 255;
                self.frame.pixels[v_index + 3] = 255;
            }
        }
        
        // 白い枠線を描画
        for i in 0..SCREEN_WIDTH {
            // 上端
            let top_index = i * 4;
            if top_index + 3 < self.frame.pixels.len() {
                self.frame.pixels[top_index] = 255;
                self.frame.pixels[top_index + 1] = 255;
                self.frame.pixels[top_index + 2] = 255;
                self.frame.pixels[top_index + 3] = 255;
            }
            
            // 下端
            let bottom_index = ((SCREEN_HEIGHT - 1) * SCREEN_WIDTH + i) * 4;
            if bottom_index + 3 < self.frame.pixels.len() {
                self.frame.pixels[bottom_index] = 255;
                self.frame.pixels[bottom_index + 1] = 255;
                self.frame.pixels[bottom_index + 2] = 255;
                self.frame.pixels[bottom_index + 3] = 255;
            }
        }
        
        for i in 0..SCREEN_HEIGHT {
            // 左端
            let left_index = (i * SCREEN_WIDTH) * 4;
            if left_index + 3 < self.frame.pixels.len() {
                self.frame.pixels[left_index] = 255;
                self.frame.pixels[left_index + 1] = 255;
                self.frame.pixels[left_index + 2] = 255;
                self.frame.pixels[left_index + 3] = 255;
            }
            
            // 右端
            let right_index = (i * SCREEN_WIDTH + SCREEN_WIDTH - 1) * 4;
            if right_index + 3 < self.frame.pixels.len() {
                self.frame.pixels[right_index] = 255;
                self.frame.pixels[right_index + 1] = 255;
                self.frame.pixels[right_index + 2] = 255;
                self.frame.pixels[right_index + 3] = 255;
            }
        }
        
        // パターンテーブルのタイルを表示するテスト（左上隅に16x16タイル）
        for tile_y in 0..16 {
            for tile_x in 0..16 {
                let tile_index = tile_y * 16 + tile_x;
                self.draw_chr_tile(tile_x * 8, tile_y * 8, tile_index, 0); // パレット0を使用
            }
        }
        
        println!("初期テストパターン描画完了！");
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
        
        // テスト用のCHR-RAMデータをセットアップ（パターンテーブルデータをシミュレート）
        self.init_test_chr_data();
        
        // 初期状態のフラグを設定
        self.ctrl.set_bits(0x90);  // NMI有効化など
        // マスクレジスタを更新して背景とスプライトを表示
        self.mask.set_bits(0x1E);  // 背景とスプライトを有効化（0x1EはBGとスプライト両方有効）
        
        println!("PPU Reset complete - Blue color scheme initialized");
        println!("PPU Mask Register: {:02X} (BG enabled: {}, Sprites enabled: {})",
                 self.mask.bits(), self.mask.show_background(), self.mask.show_sprites());
        println!("PPU Control Register: {:02X} (NMI enabled: {}, Pattern Table: ${:04X})",
                 self.ctrl.bits(), 
                 self.ctrl.generate_nmi(), 
                 if self.ctrl.background_pattern_addr() == 0 { 0x0000 } else { 0x1000 });
        
        // テストパターンを描画（デバッグ用）
        self.init_test_pattern();
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
        
        println!("Test CHR-RAM and nametable data initialized");
    }

    // PPUの1サイクル分の更新を行う
    pub fn step(&mut self) {
        // フレームが完了したかどうかをチェック
        if self.frame_complete {
            // フレーム完了時にデバッグ出力を追加
            if self.frame_counter % 60 == 0 {
                println!("PPU: フレーム {}完了。パレットRAM状態: [{:02X}, {:02X}, {:02X}, {:02X}]", 
                    self.frame_counter,
                    self.palette_ram[0], self.palette_ram[1], self.palette_ram[2], self.palette_ram[3]);
                
                // CHR-ROMの最初の数バイトを表示（パターンテーブルのデータが正しいか確認）
                println!("CHR-ROM先頭データ: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                         self.chr_ram[0], self.chr_ram[1], self.chr_ram[2], self.chr_ram[3],
                         self.chr_ram[4], self.chr_ram[5], self.chr_ram[6], self.chr_ram[7]);
                
                // レンダリング情報のサマリーを出力
                println!("PPU レンダリング状態サマリー:");
                println!("  コントロール: {:02X}, マスク: {:02X}, ステータス: {:02X}",
                         self.ctrl.bits(), self.mask.bits(), self.status.bits());
                println!("  背景表示: {}, スプライト表示: {}, NMI有効: {}",
                         self.mask.show_background(), self.mask.show_sprites(), self.ctrl.generate_nmi());
                println!("  VRAMアドレス: ${:04X}, 一時アドレス: ${:04X}",
                         self.vram_addr.addr(), self.temp_vram_addr.addr());
                println!("  パターンテーブル: ${:04X}", if self.ctrl.background_pattern_addr() == 0 { 0x0000 } else { 0x1000 });
                
                // テスト - フレームの一部をデバッグカラーで塗りつぶす
                self.draw_debug_pattern();

                // 毎フレーム、強制的にテストパターンを描画（デバッグ用）
                if self.frame_counter < 10 {
                    self.init_test_pattern();
                    println!("強制的にテストパターンを描画しました（デバッグ用）");
                }
            }
            
            // フレーム完了フラグをリセット
            self.frame_complete = false;
            return;
        }
        
        // レンダリングが有効かどうかをチェック
        let rendering_enabled = self.mask.show_background() || self.mask.show_sprites();
        
        // レンダリング有効フラグが変わったときにデバッグ出力
        static mut PREV_RENDERING: bool = false;
        let cur_rendering = rendering_enabled;
        unsafe {
            if cur_rendering != PREV_RENDERING {
                println!("PPU: レンダリング状態が変更: {} -> {}", PREV_RENDERING, cur_rendering);
                PREV_RENDERING = cur_rendering;
            }
        }

        // 背景タイル情報の取得
        if (self.scanline >= 0 && self.scanline < 240) || self.scanline == 261 {
            // 背景シフトレジスタの更新（1ドットごとに1ビットずつシフト）
            if (self.cycle >= 1 && self.cycle <= 256) || (self.cycle >= 328 && self.cycle <= 340) {
                if self.mask.show_background() {
                    // パターンシフトレジスタを1ビット左シフト
                    self.bg_shifter_pattern_lo <<= 1;
                    self.bg_shifter_pattern_hi <<= 1;
                    self.bg_shifter_attrib_lo <<= 1;
                    self.bg_shifter_attrib_hi <<= 1;
                }
            }
        }

        // 可視領域のレンダリング（0-239ライン、0-255サイクル）
        if self.scanline < 240 && self.cycle < 256 && rendering_enabled {
            let x = self.cycle as usize;
            // scanlineはisizeなので、負の値の場合は0にクリップ
            let y = if self.scanline >= 0 { self.scanline as usize } else { 0 };
            
            // PPUアドレス計算（タイルベースのレンダリング）
            // ネームテーブルベースアドレス（VRAM上の位置）
            let nametable_base = 0x2000 | (self.vram_addr.addr() & 0x0FFF);
            
            // タイル座標の計算
            let tile_x = self.vram_addr.coarse_x() as u16;
            let tile_y = self.vram_addr.coarse_y() as u16;
            let fine_y = self.vram_addr.fine_y() as u16;
            
            // デバッグ出力
            if self.cycle == 0 && self.scanline % 20 == 0 {
                println!("PPU Debug - Tile coords: x={}, y={}, fine_y={}, nametable_base=${:04X}", 
                         tile_x, tile_y, fine_y, nametable_base);
                
                // マスクレジスタの状態も出力
                println!("PPU Debug - Mask: {:02X} (BG:{}, Sprites:{})", 
                         self.mask.bits(), 
                         self.mask.show_background(), 
                         self.mask.show_sprites());
            }
            
            // タイルインデックスを取得（安全な計算）
            let tile_idx = (tile_y as u16).wrapping_mul(32).wrapping_add(tile_x);
            
            let tile_addr = nametable_base + tile_idx;
            
            // タイルIDを読み取る（VRAMアクセス）
            let tile_id = self.read_vram(tile_addr);
            
            // 属性テーブルアドレスの計算（安全にu16で計算）
            let attr_x = tile_x >> 2;
            let attr_y = tile_y >> 2;
            // シフト演算する代わりにより安全な足し算を使用
            let attr_shift = attr_y.wrapping_mul(8).wrapping_add(attr_x);
            
            // デバッグ出力
            if self.cycle == 0 && self.scanline % 20 == 0 {
                println!("PPU Debug - Attr calc: attr_x={}, attr_y={}, shift=${:04X}", 
                         attr_x, attr_y, attr_shift);
            }
            
            let attribute_addr = 0x23C0 | (self.vram_addr.addr() & 0x0C00) | attr_shift;
            
            // 属性値を読み取る
            let attribute = self.read_vram(attribute_addr);
            
            // 属性内のサブセクションを選択（u16で安全に計算）
            let shift = ((tile_y & 0x02) << 1) | (tile_x & 0x02);
            let palette_idx = (attribute >> shift) & 0x03;
            
            // パターンテーブルアドレスの計算
            // デバッグ用にパターンテーブルの計算を表示
            let pattern_base_addr = if self.ctrl.background_pattern_addr() == 0 {
                0x0000_u16
            } else {
                0x1000_u16
            };
            
            // デバッグ出力を強化
            if self.cycle == 0 && self.scanline % 20 == 0 {
                println!("PPU Debug - Pattern calc: tile_id={:02X}, pattern_base=${:04X}", 
                         tile_id, pattern_base_addr);
                println!("PPU Debug - Tile info: tile_x={}, tile_y={}, fine_y={}", 
                         tile_x, tile_y, fine_y);
            }
            
            // オーバーフロー対策: u16型の範囲内で計算
            let tile_id_u16 = tile_id as u16;
            let fine_y_u16 = fine_y as u16;
            
            // タイルオフセットの計算（タイル1つあたり16バイト）
            let tile_offset = tile_id_u16.wrapping_mul(16);
            
            // パターンテーブル内でのアドレス計算（完全に分解して計算）
            let lo_pattern_addr = pattern_base_addr.wrapping_add(tile_offset).wrapping_add(fine_y_u16);
            let hi_pattern_addr = lo_pattern_addr.wrapping_add(8);
            
            if self.cycle == 0 && self.scanline % 20 == 0 {
                println!("PPU Debug - Pattern addr: lo=${:04X}, hi=${:04X}, tile_offset=${:04X}", 
                         lo_pattern_addr, hi_pattern_addr, tile_offset);
            }
            
            // パターンデータ（タイルのピクセルデータ）を読み取る
            let pattern_lo = self.read_vram(lo_pattern_addr);
            let pattern_hi = self.read_vram(hi_pattern_addr);
            
            // パターンデータをデバッグ表示（定期的に）
            if self.cycle == 0 && self.scanline % 60 == 0 {
                println!("PPU Debug - Pattern data: lo=${:02X}, hi=${:02X} (tile_id=${:02X})", 
                         pattern_lo, pattern_hi, tile_id);
            }
            
            // 水平反転でないので、左から右に処理
            let fine_x = (7 - (x % 8)) as u8; // タイル内の水平位置（反転して処理）
            
            // パターンからピクセル値を取得
            let lo_bit = (pattern_lo >> fine_x) & 0x01;
            let hi_bit = (pattern_hi >> fine_x) & 0x01;
            let pixel_value = (hi_bit << 1) | lo_bit;
            
            // ピクセルデータをデバッグ表示（スパース表示）
            if (x % 32 == 0) && (y % 32 == 0) {
                println!("PPU Debug - Pixel at ({},{}) = {} (palette: {})", 
                         x, y, pixel_value, palette_idx);
            }
            
            // 背景色の場合（0）と非背景色の場合で処理分け
            if pixel_value == 0 {
                // 背景色（ユニバーサルバックグラウンド）
                let color_addr = 0;
                let color_idx = self.palette_ram[color_addr] as usize;
                let (r, g, b) = NES_PALETTE[color_idx & 0x3F];
                
                // ランダムな位置でデバッグ出力
                if (x % 64 == 0) && (y % 64 == 0) {
                    println!("PPU Debug - 背景色ピクセル({},{}) = パレット[{}]=${:02X} -> RGB({},{},{})", 
                             x, y, color_addr, color_idx, r, g, b);
                }
                
                let idx = (y * SCREEN_WIDTH + x) * 4;
                if idx + 3 < self.frame.pixels.len() {
                    self.frame.pixels[idx] = r;
                    self.frame.pixels[idx + 1] = g;
                    self.frame.pixels[idx + 2] = b;
                    self.frame.pixels[idx + 3] = 255;
                }
            } else {
                // パレットインデックスの計算
                let color_addr = (palette_idx * 4 + pixel_value as u8) as usize;
                let color_idx = self.palette_ram[color_addr] as usize;
                let (r, g, b) = NES_PALETTE[color_idx & 0x3F];
                
                // カラーデータをデバッグ表示（スパース表示）
                if (x % 32 == 0) && (y % 32 == 0) {
                    println!("PPU Debug - 前景色ピクセル({},{}) = パレット[{}]=${:02X} -> RGB({},{},{}), pixel_value={}, palette_idx={}", 
                             x, y, color_addr, color_idx, r, g, b, pixel_value, palette_idx);
                }
                
                let idx = (y * SCREEN_WIDTH + x) * 4;
                if idx + 3 < self.frame.pixels.len() {
                    self.frame.pixels[idx] = r;
                    self.frame.pixels[idx + 1] = g;
                    self.frame.pixels[idx + 2] = b;
                    self.frame.pixels[idx + 3] = 255;
                }
            }
        }
        
        // PPUサイクルを進める
        self.cycle += 1;
        if self.cycle >= 341 {
            self.cycle = 0;
            self.scanline += 1;
            
            if self.scanline >= 261 {
                self.scanline = 0;
                self.frame_complete = true;
                self.frame_counter += 1;
            }
        }
        
        // VBlankフラグとNMI生成
        if self.scanline == 241 && self.cycle == 1 {
            self.status.set_vblank_started(true);
            if self.ctrl.generate_nmi() {
                self.nmi_line_low = false;
            }
        }
        else if self.scanline == 261 && self.cycle == 1 {
            // VBlank終了時（プリレンダースキャンライン）
            self.status.set_vblank_started(false);
            self.status.set_sprite_zero_hit(false);
            self.status.set_sprite_overflow(false);
            self.nmi_line_low = true;
        }

        // レンダリングが有効な場合、水平・垂直スクロールの処理を行う
        if rendering_enabled {
            // 各スキャンラインの可視領域の最後でx座標をインクリメント
            if self.cycle == 256 && self.scanline < 240 {
                self.increment_scroll_x();
            }
            
            // 各スキャンラインの特定の位置でy座標をインクリメント
            if self.cycle == 257 && self.scanline < 240 {
                self.increment_scroll_y();
            }
            
            // PPUアドレスの水平ビットを更新（水平スクロールをリセット）
            if self.cycle == 257 && self.scanline <= 239 {
                self.transfer_address_x();
            }
            
            // 垂直スクロールの更新（プリレンダーラインの特定のサイクルで）
            if self.scanline == 261 && self.cycle >= 280 && self.cycle <= 304 {
                self.transfer_address_y();
            }
        }
    }
    
    // PPUサイクルごとのスクロール更新
    fn increment_scroll_x(&mut self) {
        if self.mask.show_background() || self.mask.show_sprites() {
            if self.vram_addr.inc_coarse_x() {
                // coarse_xが一周した場合のみnametableを切り替える
                println!("Horizontal nametable switch");
            }
        }
    }
    
    fn increment_scroll_y(&mut self) {
        if self.mask.show_background() || self.mask.show_sprites() {
            if self.vram_addr.inc_fine_y() {
                // fine_yが一周した場合のみcoarse_yをインクリメント
                println!("Vertical scroll increment");
            }
        }
    }
    
    fn transfer_address_x(&mut self) {
        if self.mask.show_background() || self.mask.show_sprites() {
            self.vram_addr.copy_horizontal_bits(&self.temp_vram_addr);
        }
    }
    
    fn transfer_address_y(&mut self) {
        if self.mask.show_background() || self.mask.show_sprites() {
            self.vram_addr.copy_vertical_bits(&self.temp_vram_addr);
        }
    }

    // VRAMアドレスのミラーリングを行う
    pub fn mirror_vram_addr(&self, addr: u16, mirroring: Mirroring) -> usize {
        let addr = addr as usize & 0x3FFF; // 14ビットに制限
        
        if addr <= 0x1FFF {
            // パターンテーブル領域 ($0000-$1FFF)
            return addr;
        } else if addr <= 0x3EFF {
            // ネームテーブル領域 ($2000-$3EFF)
            // 実際の2KBのVRAMにマッピング
            let addr = addr & 0x0FFF; // $2000-$2FFFの範囲に変換
            let table = addr / 0x0400; // テーブル番号 (0-3)
            
            match mirroring {
                Mirroring::Horizontal => {
                    // 水平ミラーリング: 0,1は0へ、2,3は1へマップ
                    let mirrored_table = table & 0x01;
                    (mirrored_table * 0x0400) | (addr & 0x03FF)
                }
                Mirroring::Vertical => {
                    // 垂直ミラーリング: 0,2は0へ、1,3は1へマップ
                    let mirrored_table = table & 0x02;
                    (mirrored_table * 0x0200) | (addr & 0x03FF)
                }
                Mirroring::SingleScreenLower => {
                    // 1画面ミラーリング (下部): すべて0へマップ
                    addr & 0x03FF
                }
                Mirroring::SingleScreenUpper => {
                    // 1画面ミラーリング (上部): すべて1へマップ
                    0x0400 | (addr & 0x03FF)
                }
                Mirroring::FourScreen => {
                    // 4画面: そのまま
                    addr
                }
            }
        } else {
            // パレットRAM領域 ($3F00-$3FFF)
            let palette_addr = addr & 0x1F;
            // パレットのミラーリング
            if palette_addr == 0x10 || palette_addr == 0x14 || 
               palette_addr == 0x18 || palette_addr == 0x1C {
                return palette_addr & 0x0F;
            } else {
                return palette_addr;
            }
        }
    }

    // VRAMからデータを読み取る
    pub fn read_vram(&self, addr: u16) -> u8 {
        let addr_masked = addr & 0x3FFF; // 14ビットアドレス空間に制限
        
        if addr_masked <= 0x1FFF {
            // パターンテーブル ($0000-$1FFF)
            // カートリッジのCHR-ROMまたはCHR-RAMから読み取り
            return self.chr_ram[addr_masked as usize];
        } else if addr_masked <= 0x3EFF {
            // ネームテーブル ($2000-$2FFF), ミラー ($3000-$3EFF)
            // 内部実装ではMirroringを持っていないため、単純に下位11ビットを使う
            let mirrored_addr = (addr_masked as usize & 0x0FFF) % self.vram.len();
            return self.vram[mirrored_addr];
        } else {
            // パレットデータ ($3F00-$3FFF)
            return self.read_palette_ram(addr);
        }
    }

    // Placeholder for PPU reads - Replace with Bus calls eventually
    fn ppu_read_placeholder(&self, addr: u16) -> u8 {
        match addr & 0x3FFF {
            0x0000..=0x1FFF => self.chr_ram.get(addr as usize).copied().unwrap_or(0), // Use CHR RAM
            0x2000..=0x3EFF => { // Simulate VRAM read with mirroring
                let mirrored_addr = self.mirror_vram_addr(addr, Mirroring::FourScreen); // Placeholder mirroring
                 self.vram.get(mirrored_addr).copied().unwrap_or(0)
            }
            0x3F00..=0x3FFF => self.read_palette_ram(addr), // Read directly from palette RAM
            _ => 0,
        }
    }

    // Direct read from palette RAM, handling mirroring
    fn read_palette_ram(&self, addr: u16) -> u8 {
        let index = (addr & 0x1F) as usize;
        
        // ミラーリング処理
        let mirrored_index = if index == 0x10 || index == 0x14 || 
                               index == 0x18 || index == 0x1C {
            index & 0x0F
        } else {
            index
        };
        
        self.palette_ram[mirrored_index]
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

    // PPUCTRLレジスタ ($2000) の書き込み処理
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
    pub fn read_oam_data(&self) -> u8 { self.oam_data[self.oam_addr as usize] }
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

    // テスト - フレームの一部をデバッグカラーで塗りつぶす
    fn draw_debug_pattern(&mut self) {
        // テスト - フレームの一部をデバッグカラーで塗りつぶす
        for y in 0..16 {
            for x in 0..16 {
                let idx = (y * SCREEN_WIDTH + x) * 4;
                // インデックスが有効範囲内か確認
                if idx + 3 < self.frame.pixels.len() {
                    self.frame.pixels[idx] = 255;
                    self.frame.pixels[idx + 1] = 0;
                    self.frame.pixels[idx + 2] = 0;
                    self.frame.pixels[idx + 3] = 255;
                }
            }
        }
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
