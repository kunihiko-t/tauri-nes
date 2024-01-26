use serde::{Serialize, Deserialize};


struct Controller {
    a_button: bool,
    b_button: bool,
    start: bool,
    select: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}


#[derive(Serialize, Deserialize)]
pub(crate) struct InputData {
    a_button: bool,
    b_button: bool,
    start: bool,
    select: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}


impl Controller {
    pub fn new() -> Self {
        Self {
            a_button: false,
            b_button: false,
            start: false,
            select: false,
            up: false,
            down: false,
            left: false,
            right: false,
        }
    }

    pub fn update(&mut self, input_data: InputData) {
        // `input_data` に基づいてコントローラーの状態を更新
    }
    // 他のメソッド（例: ボタンの状態を更新するメソッドなど）
}