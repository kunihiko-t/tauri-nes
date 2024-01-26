use serde::Serialize;

use crate::ram::Memory;
use crate::debugger::Debugger;

const CARRY_FLAG: u8 = 0b00000001;
const ZERO_FLAG: u8 = 0b00000010;
const INTERRUPT_DISABLE_FLAG: u8 = 0b00000100;
const DECIMAL_MODE_FLAG: u8 = 0b00001000;
const BREAK_FLAG: u8 = 0b00010000;
const UNUSED_FLAG: u8 = 0b00100000;
const OVERFLOW_FLAG: u8 = 0b01000000;
const NEGATIVE_FLAG: u8 = 0b10000000;
// 6502 CPUの状態を保持する構造体
#[derive(Serialize)]
pub struct Cpu6502 {
    accumulator: u8,
    x_register: u8,
    y_register: u8,
    status: u8,
    program_counter: u16,
    stack_pointer: u8,
    memory: Memory,
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


pub struct Registers {
    accumulator: u8,
    x_index: u8,
    y_index: u8,
    status: u8,
    program_counter: u16,
    stack_pointer: u8,
}

impl Registers {
    fn new() -> Self {
        Self {
            accumulator: 0,
            x_index: 0,
            y_index: 0,
            status: 0,
            program_counter: 0,
            stack_pointer: 0,
        }
    }
}


impl Cpu6502 {
    pub(crate) fn new() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            status: 0x24, // Set unused and break flags
            program_counter: 0xC000, // Common start address for NES
            stack_pointer: 0,
            memory: Memory::new()
        }
    }


    pub fn step(&mut self, memory: &mut Memory, debugger: &Debugger) {
        if debugger.check_breakpoint(self.program_counter) {
            // ブレークポイントに達した場合の処理
        }

        self.execute(memory);
        // 必要に応じて追加のデバッグ情報を表示
    }
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


    // 即値（Immediate）アドレッシング
    fn immediate(&mut self) -> u8 {
        let value = self.read(self.program_counter);
        self.program_counter += 1;
        value
    }

    // ゼロページ（Zero Page）アドレッシング
    fn zero_page(&mut self) -> u16 {
        let addr = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        addr
    }



    // 絶対（Absolute）アドレッシング
    fn absolute(&mut self) -> u16 {
        let low = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        let high = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        (high << 8) | low
    }

    // メモリから値を読み取る
    fn read(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    // メモリに値を書き込む
    fn write(&mut self, addr: u16, data: u8) {
        self.memory.write(addr, data);
    }

    fn jmp(&mut self, addr: u16) {
        self.program_counter = addr;
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


    // フラグの更新
    fn update_zero_and_negative_flags(&mut self, value: u8) {
        self.status = (self.status & !(ZERO_FLAG | NEGATIVE_FLAG))
            | ((value == 0) as u8 * ZERO_FLAG)
            | (((value & 0x80) != 0) as u8 * NEGATIVE_FLAG);
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
            if self.program_counter == 0x1234 {
                break;
            }
            // ここにエミュレーションの実行を停止する条件を追加する
            // 例: 特定のプログラムカウンタ値に到達した場合、または特定の命令を実行した後
        }
    }

    pub fn inspect_memory(&self, memory: &Memory, addr: u16) -> u8 {
        memory.read(addr)
    }


    pub fn inspect_registers(&self) -> Registers {
        Registers {
            accumulator: self.accumulator,
            x_index: self.x_register,
            y_index: self.y_register,
            status: self.status,
            program_counter: self.program_counter,
            stack_pointer: self.stack_pointer,
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
