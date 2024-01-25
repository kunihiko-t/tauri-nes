use serde::Serialize;

use crate::ram::Memory;
// 6502 CPUの状態を保持する構造体
#[derive(Serialize)]
pub struct Cpu6502 {
    accumulator: u8,
    x_register: u8,
    y_register: u8,
    status: u8,
    program_counter: u16,
    memory: Vec<u8>,
}


// 6502 CPUの状態を保持する構造体
#[derive(Serialize)]
#[derive(Clone)]
pub struct CpuState {
    accumulator: u8,
    x_register: u8,
    y_register: u8,
    status: u8,
    program_counter: u16,
    // 他のレジスタやフラグがあればここに追加
}


impl Cpu6502 {
    pub(crate) fn new() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            status: 0x24, // Set unused and break flags
            program_counter: 0xC000, // Common start address for NES
            memory: vec![0; 65536], // 64KB of memory
        }
    }

    // ここにCPUの命令を実行するメソッドを追加...
}

// 6502 CPUの命令セットを実装する
impl Cpu6502 {

    // LDA: Load Accumulator
    fn lda(&mut self, value: u8) {
        self.accumulator = value;
        self.update_zero_and_negative_flags(self.accumulator);
    }

    // STA: Store Accumulator
    fn sta(&mut self, addr: u16, memory: &mut Memory) {
        memory.write(addr, self.accumulator);
    }

    // BEQ: Branch if Equal
    fn beq(&mut self, offset: i8) {
        if self.status & 0x02 != 0 {  // ゼロフラグがセットされている場合
            self.program_counter = ((self.program_counter as i16) + offset as i16) as u16;
        }
    }

    pub(crate) fn get_status(&self) -> CpuState {
        CpuState {
            accumulator: self.accumulator,
            x_register: self.x_register,
            y_register: self.y_register,
            status: self.status,
            program_counter: self.program_counter,
        }
    }

    // ゼロフラグとネガティブフラグを更新
    fn update_zero_and_negative_flags(&mut self, value: u8) {
        if value == 0 {
            self.status |= 0x02; // ゼロフラグをセット
        } else {
            self.status &= !0x02; // ゼロフラグをクリア
        }

        if value & 0x80 != 0 {
            self.status |= 0x80; // ネガティブフラグをセット
        } else {
            self.status &= !0x80; // ネガティブフラグをクリア
        }
    }

    // 命令のフェッチ、デコード、実行
    fn execute(&mut self, memory: &mut Memory) {
        let opcode = memory.read(self.program_counter);
        self.program_counter = self.program_counter.wrapping_add(1);

        match opcode {
            0xA9 => { // LDA Immediate
                let value = memory.read(self.program_counter);
                self.program_counter += 1;
                self.lda(value);
            },
            0x00 => {
                // BRK命令の処理
                // 例: スタックにプログラムカウンタとステータスレジスタをプッシュし、割り込みベクタから新しいPCをロード
            },
            // 他の命令...
            _ => unimplemented!("Opcode {:02X} is not implemented", opcode),
        }
    }
    pub fn run(&mut self, memory: &mut Memory) {
        loop {
            self.execute(memory);

            // ここにエミュレーションの実行を停止する条件を追加する
            // 例: 特定のプログラムカウンタ値に到達した場合、または特定の命令を実行した後
        }
    }

    // 他の命令も同様にメソッドとして実装...
}


impl CpuState {
    // CPU状態の新しいインスタンスを生成するための関数
    pub fn new() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            status: 0,
            program_counter: 0,
            // 他のフィールドの初期化...
        }
    }

}
