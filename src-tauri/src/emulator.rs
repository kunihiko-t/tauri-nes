use crate::cpu::Cpu6502;
use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::NesRom;
use crate::ppu::{FrameData, Ppu};
// use std::sync::{Arc, Mutex}; // Removed unused import
// use std::path::Path; // Removed unused import
// TODO: Add other necessary components like PPU, APU, Cartridge

pub struct Emulator {
    cpu: Cpu6502,
    pub bus: Bus,
    ppu: Ppu,
    // apu: Apu,
    // cartridge: Option<Cartridge>, // Removed, Bus owns the cartridge now
    cycles_this_frame: u64,
    is_running: bool,
    // rom_path: Option<String>, // Keep track of loaded ROM path if needed
    // Add other state like running status, frame count etc.
}

impl Emulator {
    pub fn new() -> Self {
        // まずCPUとPPUを初期化します（Bus内のものとは別に）
        let cpu = Cpu6502::new();
        let ppu = Ppu::new();
        
        // 次にBusを初期化（Bus内部でも別のCPUとPPUインスタンスが作成される）
        let bus = Bus::new();

        Emulator {
            cpu,           // 外部CPUインスタンス
            bus,           // メインBus
            ppu,           // 外部PPUインスタンス
            cycles_this_frame: 0,
            is_running: true,
        }
    }

    // Load ROM from a file path
    pub fn load_rom(&mut self, file_path: &str) -> Result<(), String> {
        // 1. Use NesRom::from_file to read and parse the ROM directly
        println!("Attempting to load and parse ROM from: {}", file_path);
        let nes_rom = NesRom::from_file(file_path)
            .map_err(|e| format!("Failed to load or parse NES ROM '{}': {}", file_path, e))?;
        // println!("Successfully read {} bytes.", rom_data.len()); // Removed

        // 2. Parsing is done by NesRom::from_file
        // let nes_rom = NesRom::new(rom_data) // Removed
        //      .map_err(|e| format!("Failed to parse NES ROM '{}': {}", file_path, e))?;
        println!("Successfully parsed ROM header. Mapper: {}", nes_rom.mapper_id);

        // 3. Create Cartridge
        // Extract necessary fields from nes_rom
        let prg_rom = nes_rom.prg_rom.clone(); // Clone data to pass ownership
        let chr_rom = nes_rom.chr_rom.clone();
        let mapper_id = nes_rom.mapper_id;
        let mirroring_flags = nes_rom.mirroring.into_flags(); // Get mirroring flags

        let cartridge = Cartridge::new(
            prg_rom,
            chr_rom,
            mapper_id,
            mirroring_flags,
        )?; // Propagate error from Cartridge::new
        println!("Cartridge created successfully.");

        // 4. Insert Cartridge into Bus
        //    This replaces the previous cartridge if any.
        self.bus.insert_cartridge(cartridge);
        println!("Cartridge inserted into bus.");

        // 5. Reset CPU to apply changes and read reset vector from cartridge
        self.cpu.reset(&mut self.bus);
        // Correct program_counter access
        println!("CPU reset. PC starting at: {:#04X}", self.cpu.inspect().registers.program_counter);

        println!("ROM '{}' loaded successfully.", file_path);
        Ok(())
    }

    // Runs the emulator for approximately one frame
    pub fn run_frame(&mut self) -> u32 {
        // 1フレーム分のサイクルを実行
        let mut cycles_this_frame = 0u32;

        // 実行前のサイクル数を記録
        let start_cycles = self.bus.total_cycles;
        
        // フレーム完了フラグをリセット
        self.bus.reset_frame_complete();
        
        // 静的なフレームカウンタ
        static mut FRAME_COUNTER: u32 = 0;
        
        // BRKカウンターとリセット検出のための変数
        static mut CONSECUTIVE_BRK_COUNT: u32 = 0;
        static mut LAST_PC: u16 = 0;
        static mut IN_TEST_MODE: bool = true;
        
        // フレームが完了するまでCPUサイクルを実行
        loop {
            // 現在のPCを保存
            let current_pc = self.bus.get_cpu_state().registers.program_counter;
            
            // BRKの繰り返しを検出
            unsafe {
                // 現在のオペコードを取得 - バスからの直接読み取り
                let current_opcode = self.bus.debug_read(current_pc);
                if current_opcode == 0x00 { // BRK
                    if current_pc == LAST_PC {
                        CONSECUTIVE_BRK_COUNT += 1;
                    } else {
                        CONSECUTIVE_BRK_COUNT = 0;
                    }
                    
                    // 連続BRKが多すぎる場合、テストモードを有効化
                    if CONSECUTIVE_BRK_COUNT > 5 {
                        println!("連続BRK検出: テストモードに切り替えます");
                        IN_TEST_MODE = true;
                        
                        // ブート用の安全なアドレスへジャンプ - CPU経由でリセットする
                        println!("CPU強制リセット実行");
                        self.reset(); // CPUをリセット
                        CONSECUTIVE_BRK_COUNT = 0;
                    }
                } else {
                    CONSECUTIVE_BRK_COUNT = 0;
                }
                
                LAST_PC = current_pc;
            }
            
            // CPUクロックを実行
            let cycles = self.bus.clock();
            
            // サイクルカウンタを更新（オーバーフローを防止するためwrapping_addを使用）
            cycles_this_frame = cycles_this_frame.wrapping_add(cycles as u32);
            
            // フレーム完了チェック
            if self.bus.is_frame_complete() {
                // フレーム完了フラグをリセット
                self.bus.reset_frame_complete();
                break;
            }
            
            // 安全装置：過剰なサイクル数でループを抜ける（オーバーフロー対策）
            if cycles_this_frame > 100_000 {
                println!("WARNING: Safety break - excessive cycles: {}", cycles_this_frame);
                break;
            }
        }

        // デバッグ用：テストモード時のみフレームバッファに直接色を設定
        unsafe {
            if IN_TEST_MODE {
                // フレームカウンタを安全に増加
                let frame_count = {
                    FRAME_COUNTER = FRAME_COUNTER.wrapping_add(1);
                    FRAME_COUNTER
                };
                
                println!("テストモード - デバッグ画面描画 (フレーム {})", frame_count);
                // フレームバッファを取得
                let mut frame = self.bus.get_ppu_frame();
                let width = frame.width;
                let height = frame.height;
                
                // 背景を黒にクリア
                for i in 0..frame.pixels.len() {
                    frame.pixels[i] = 0;
                }
                
                // 市松模様パターン - フレームごとに色が変化
                for y in 0..height {
                    for x in 0..width {
                        let idx = (y * width + x) * 4;
                        
                        // 市松模様パターン - フレームカウントによって色の強度を変化
                        let color_phase = ((frame_count % 120) as f32 / 60.0) * std::f32::consts::PI;
                        let intensity_red = ((color_phase.sin() + 1.0) / 2.0 * 255.0) as u8;
                        let intensity_blue = ((color_phase.cos() + 1.0) / 2.0 * 255.0) as u8;
                        
                        let is_even_x = x % 32 < 16;
                        let is_even_y = y % 32 < 16;
                        
                        if (is_even_x && is_even_y) || (!is_even_x && !is_even_y) {
                            // 赤色 - 強度が変化
                            frame.pixels[idx] = intensity_red;     // R
                            frame.pixels[idx + 1] = 0;             // G
                            frame.pixels[idx + 2] = 0;             // B
                            frame.pixels[idx + 3] = 255;           // A
                        } else {
                            // 青色 - 強度が変化
                            frame.pixels[idx] = 0;                 // R
                            frame.pixels[idx + 1] = 0;             // G
                            frame.pixels[idx + 2] = intensity_blue; // B
                            frame.pixels[idx + 3] = 255;           // A
                        }
                    }
                }
                
                // 動く円を描画
                let circle_radius = 20;
                let orbit_radius = 50;
                let orbit_angle = (frame_count as f32 / 30.0) * std::f32::consts::PI;
                let circle_x = (width as f32 / 2.0 + orbit_radius as f32 * orbit_angle.cos()) as usize;
                let circle_y = (height as f32 / 2.0 + orbit_radius as f32 * orbit_angle.sin()) as usize;
                
                for y in 0..height {
                    for x in 0..width {
                        let dx = x as isize - circle_x as isize;
                        let dy = y as isize - circle_y as isize;
                        let distance_squared = dx * dx + dy * dy;
                        
                        if distance_squared <= (circle_radius * circle_radius) as isize {
                            let idx = (y * width + x) * 4;
                            // 白い円
                            frame.pixels[idx] = 255;     // R
                            frame.pixels[idx + 1] = 255; // G
                            frame.pixels[idx + 2] = 255; // B
                            frame.pixels[idx + 3] = 255; // A
                        }
                    }
                }
                
                // 画面中央に十字を描画
                let center_x = width / 2;
                let center_y = height / 2;
                
                for i in 0..width {
                    let h_idx = (center_y * width + i) * 4;
                    frame.pixels[h_idx] = 255;     // R
                    frame.pixels[h_idx + 1] = 255; // G
                    frame.pixels[h_idx + 2] = 255; // B
                    frame.pixels[h_idx + 3] = 255; // A
                }
                
                for i in 0..height {
                    let v_idx = (i * width + center_x) * 4;
                    frame.pixels[v_idx] = 255;     // R
                    frame.pixels[v_idx + 1] = 255; // G
                    frame.pixels[v_idx + 2] = 255; // B
                    frame.pixels[v_idx + 3] = 255; // A
                }
                
                // テストモードステータスを表示
                let status_str = format!("テストモード");
                
                // ステータス表示を上部に追加
                let status_x = 10;
                let status_y = 10;
                let status_color = [0, 255, 0, 255]; // 緑色
                
                // ステータスメッセージ描画（赤枠で囲む）
                for dy in 0..14 {
                    for dx in 0..100 {
                        let px = status_x + dx;
                        let py = status_y + dy;
                        
                        if px < width && py < height {
                            let idx = (py * width + px) * 4;
                            
                            // 枠を描画
                            if dx == 0 || dx == 99 || dy == 0 || dy == 13 {
                                frame.pixels[idx] = 255;     // R
                                frame.pixels[idx + 1] = 0;   // G
                                frame.pixels[idx + 2] = 0;   // B
                                frame.pixels[idx + 3] = 255; // A
                            }
                            
                            // "テストモード" の文字を中に表示（簡易的）
                            if dy == 7 && dx >= 20 && dx < 80 {
                                frame.pixels[idx] = status_color[0];     // R
                                frame.pixels[idx + 1] = status_color[1]; // G
                                frame.pixels[idx + 2] = status_color[2]; // B
                                frame.pixels[idx + 3] = status_color[3]; // A
                            }
                        }
                    }
                }
                
                // デジタル時計を表示
                let seconds = frame_count / 60; // 60FPSと仮定
                let minutes = seconds / 60;
                let hours = minutes / 60;
                let time_str = format!("{:02}:{:02}:{:02}", hours % 24, minutes % 60, seconds % 60);
                
                // テストモード終了ボタン情報
                let help_str = "テストモード: Space でROM実行に切替";
                
                // 簡易的なデジタル表示を実装（実際のフォント描画ではなく、簡易的な表現）
                let text_x = 10;
                let text_y = 10;
                let text_color = [255, 255, 0, 255]; // 黄色
                
                for (i, c) in time_str.chars().enumerate() {
                    let char_x = text_x + i * 8;
                    match c {
                        '0'..='9' | ':' => {
                            // 数字と区切り文字を簡易的に表示（ドット表現）
                            let dots = match c {
                                '0' => [1,1,1,1,0,1,1,0,1,1,0,1,1,1,1],
                                '1' => [0,0,1,0,0,1,0,0,1,0,0,1,0,0,1],
                                '2' => [1,1,1,0,0,1,1,1,1,1,0,0,1,1,1],
                                '3' => [1,1,1,0,0,1,1,1,1,0,0,1,1,1,1],
                                '4' => [1,0,1,1,0,1,1,1,1,0,0,1,0,0,1],
                                '5' => [1,1,1,1,0,0,1,1,1,0,0,1,1,1,1],
                                '6' => [1,1,1,1,0,0,1,1,1,1,0,1,1,1,1],
                                '7' => [1,1,1,0,0,1,0,0,1,0,0,1,0,0,1],
                                '8' => [1,1,1,1,0,1,1,1,1,1,0,1,1,1,1],
                                '9' => [1,1,1,1,0,1,1,1,1,0,0,1,1,1,1],
                                ':' => [0,0,0,0,1,0,0,0,0,0,1,0,0,0,0],
                                _ => [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                            };
                            
                            // 3x5のドットマトリックスで文字を描画
                            for dy in 0..5 {
                                for dx in 0..3 {
                                    if dots[dy * 3 + dx] == 1 {
                                        let px = char_x + dx;
                                        let py = text_y + dy;
                                        if px < width && py < height {
                                            let idx = (py * width + px) * 4;
                                            frame.pixels[idx] = text_color[0];     // R
                                            frame.pixels[idx + 1] = text_color[1]; // G
                                            frame.pixels[idx + 2] = text_color[2]; // B
                                            frame.pixels[idx + 3] = text_color[3]; // A
                                        }
                                    }
                                }
                            }
                        },
                        _ => {}
                    }
                }
                
                // 画面下部にヘルプテキスト表示
                let help_text_y = height - 20;
                let text2_color = [255, 255, 255, 255]; // 白色
                
                // ヘルプテキストを描画する処理を改善（文字をピクセルアートで表示）
                let chars = "テストモード: Space キーでROM実行切替";
                for (i, _) in chars.chars().enumerate() {
                    let char_width = 8;
                    let char_start_x = text_x + i * char_width;
                    
                    // 各文字位置に長方形を描画
                    for dy in 0..10 {
                        for dx in 0..char_width-1 {
                            let px = char_start_x + dx;
                            let py = help_text_y + dy;
                            if px < width && py < height {
                                let idx = (py * width + px) * 4;
                                
                                // 文字の境界を描画（枠として表示）
                                if dx == 0 || dx == char_width-2 || dy == 0 || dy == 9 {
                                    frame.pixels[idx] = text2_color[0];     // R
                                    frame.pixels[idx + 1] = text2_color[1]; // G
                                    frame.pixels[idx + 2] = text2_color[2]; // B
                                    frame.pixels[idx + 3] = text2_color[3]; // A
                                }
                            }
                        }
                    }
                }
                
                // 画面の一番下に横線を描画
                for x in 0..width {
                    let idx = ((height - 1) * width + x) * 4;
                    frame.pixels[idx] = 128;     // R
                    frame.pixels[idx + 1] = 128; // G
                    frame.pixels[idx + 2] = 128; // B
                    frame.pixels[idx + 3] = 255; // A
                }
                
                // 更新したフレームをバスにセット
                self.bus.set_ppu_frame(frame);
            }
        }

        // 合計サイクル数を計算（オーバーフロー対策のためwrapping_sub使用）
        let total_executed = self.bus.total_cycles.wrapping_sub(start_cycles) as u32;
        println!("Frame complete at cycle: {}, cycles executed: {}", self.bus.total_cycles, total_executed);
        
        total_executed
    }

    // キー入力を処理するメソッドを追加
    pub fn handle_input(&mut self, key_code: &str, pressed: bool) {
        // スペースキーでテストモードを切り替え
        if key_code == "Space" && pressed {
            unsafe {
                // テストモードフラグを切り替え
                static mut IN_TEST_MODE: bool = true;
                IN_TEST_MODE = !IN_TEST_MODE;
                
                println!("テストモード: {}", if IN_TEST_MODE { "有効" } else { "無効" });
                
                if !IN_TEST_MODE {
                    // テストモード終了時にCPUをリセット
                    self.reset();
                }
            }
        }
        
        // 他のキー入力をコントローラーに渡す処理
        // self.bus.set_controller_button(key_code, pressed);
    }

    // Emulatorのデバッグ情報を表示
    pub fn print_debug_info(&self) {
        println!("=== エミュレータデバッグ情報 ===");
        println!("CPU状態: PC=${:04X}, A=${:02X}, X=${:02X}, Y=${:02X}, P=${:02X}, SP=${:02X}",
                self.bus.get_cpu_state().registers.program_counter,
                self.bus.get_cpu_state().registers.accumulator,
                self.bus.get_cpu_state().registers.index_x,
                self.bus.get_cpu_state().registers.index_y,
                self.bus.get_cpu_state().registers.status,
                self.bus.get_cpu_state().registers.stack_pointer);
        println!("総サイクル数: {}", self.bus.total_cycles);
        
        // PPU情報
        let ppu_frame = self.bus.get_ppu_frame();
        println!("PPUフレームサイズ: {}x{}", ppu_frame.width, ppu_frame.height);
        
        // スタック情報
        println!("スタック状態:");
        for i in 0..5 {
            let addr = 0x0100 + (self.bus.get_cpu_state().registers.stack_pointer as u16) + (i as u16);
            let value = self.bus.debug_read(addr);
            println!("  ${:04X}: ${:02X}", addr, value);
        }
        
        println!("========================");
    }

    // TODO: Implement inspect_cpu method (optional, for debugging)
    pub fn inspect_cpu(&self) -> crate::cpu::InspectState { // Make inspect public if needed
        self.cpu.inspect()
    }

    // メモリ内容のデバッグ表示を追加
    pub fn debug_memory(&self, start_addr: u16, count: u16) {
        println!("Memory dump from ${:04X} to ${:04X}:", start_addr, start_addr + count - 1);
        for i in 0..count {
            let addr = start_addr + i;
            let value = self.bus.debug_read(addr);
            print!("{:02X} ", value);
            if (i + 1) % 16 == 0 || i == count - 1 {
                println!();
            }
        }
    }

    // デバッグ機能を追加
    pub fn debug_read(&self, addr: u16) -> u8 {
        self.bus.debug_read(addr)
    }

    pub fn debug_memory_dump(&self, start_addr: u16, length: u16) {
        self.bus.debug_memory_dump(start_addr, length)
    }

    pub fn debug_disassemble(&self, start_addr: u16, num_instructions: u16) {
        self.bus.debug_disassemble(start_addr, num_instructions)
    }

    // Call the CPU's reset method via the Bus
    pub fn reset(&mut self) {
        self.bus.reset(); // Reset through the bus
        // Accessing PC needs adjustment due to InspectState structure change
        println!("CPU reset. PC starting at: {:#04X}", self.bus.get_cpu_state().registers.program_counter);
    }
}

// Default implementation for Emulator
impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}
