import { invoke } from "@tauri-apps/api/tauri";
import { open } from '@tauri-apps/api/dialog'
import { useState, useEffect } from "react";
import "./App.css";

interface CpuState {
    accumulator: number;
    x_register: number;
    y_register: number;
    status: number;
    program_counter: number;
}

function App() {
    const [cpuState, setCpuState] = useState<CpuState | null>(null);

    useEffect(() => {
        const intervalId = setInterval(() => {
            getCpuState();
        }, 1000); // 1000msごとに状態を更新

        // クリーンアップ関数
        return () => clearInterval(intervalId);
    }, []);



// キーボードイベントを処理し、Rustのバックエンドに送信する

    useEffect(() => {

        type KeyToButtonMap = {
            [key: string]: keyof ControllerState;
        };
        const keyToButtonMap: KeyToButtonMap = {
            'KeyZ': 'a_button',
            'KeyX': 'b_button',
            'Enter': 'start',
            'ShiftRight': 'select',
            'ArrowUp': 'up',
            'ArrowDown': 'down',
            'ArrowLeft': 'left',
            'ArrowRight': 'right',
        };
        type ControllerState = {
            a_button: boolean;
            b_button: boolean;
            start: boolean;
            select: boolean;
            up: boolean;
            down: boolean;
            left: boolean;
            right: boolean;
        };

        const controllerState: ControllerState = {
            a_button: false,
            b_button: false,
            start: false,
            select: false,
            up: false,
            down: false,
            left: false,
            right: false,
        };


        const handleKeyEvent = (event: KeyboardEvent, isKeyDown: boolean) => {
            const button = keyToButtonMap[event.code];
            if (button) {
                controllerState[button] = isKeyDown;
                invoke('handle_input', { inputData: controllerState });
            }
        };

        const handleKeyDown = (event: KeyboardEvent) => handleKeyEvent(event, true);
        const handleKeyUp = (event: KeyboardEvent) => handleKeyEvent(event, false);

        // イベントリスナーを追加
        document.addEventListener('keydown', handleKeyDown);
        document.addEventListener('keyup', handleKeyUp);

        // コンポーネントがアンマウントされるときにイベントリスナーを削除
        return () => {
            document.removeEventListener('keydown', handleKeyDown);
            document.removeEventListener('keyup', handleKeyUp);
        };
    }, []);

    async function getCpuState() {
        try {
            const state = await invoke<CpuState>("get_cpu_state");
            setCpuState(state);
        } catch (error) {
            console.error('Error getting CPU state:', error);
        }
    }

    function convertChrRomToRgba(chrRomData: Uint8Array, width: number, height: number): Uint8ClampedArray {
        const rgbaData = new Uint8ClampedArray(width * height * 4);

        for (let tile = 0; tile < chrRomData.length / 16; tile++) {
            for (let row = 0; row < 8; row++) {
                for (let col = 0; col < 8; col++) {
                    // 2つの平面からビットを取得
                    const plane1 = chrRomData[tile * 16 + row] & (1 << (7 - col));
                    const plane2 = chrRomData[tile * 16 + row + 8] & (1 << (7 - col));
                    const paletteIndex = ((plane1 >> (7 - col)) << 1) | (plane2 >> (7 - col));

                    // グレースケールカラーを適用（ここでは仮のパレットとして）
                    const color = paletteIndex * 85; // 0, 85, 170, 255

                    // ピクセル位置を計算
                    const x = (tile % 16) * 8 + col;
                    const y = (tile / 16 | 0) * 8 + row;

                    // RGBAデータを設定
                    const dataIndex = (y * width + x) * 4;
                    rgbaData[dataIndex] = color; // R
                    rgbaData[dataIndex + 1] = color; // G
                    rgbaData[dataIndex + 2] = color; // B
                    rgbaData[dataIndex + 3] = 255;   // A
                }
            }
        }

        return rgbaData;
    }
    async function openDialog() {
        const filePath = await open();
        console.log(filePath);

        try {
            const buffer: ArrayBuffer = await invoke<ArrayBuffer>("send_chr_rom", { filePath });
            const chrRomData = new Uint8Array(buffer);
            drawChrRom(chrRomData);
        } catch (error) {
            console.error('Error loading CHR ROM:', error);
        }
    }

    function drawChrRom(chrRomData: Uint8Array) {
        // HTMLのCanvas要素を取得
        const canvas = document.getElementById('chrCanvas') as HTMLCanvasElement;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
            throw new Error("Could not get canvas context");
        }
        // Canvasのサイズを適切に設定する（タイルの数に基づく）
        canvas.width = 128; // 16タイル分の幅
        canvas.height = (chrRomData.length / 16) * 8 / 16; // CHR ROMのタイル数に基づいた高さ

        // CHR ROMデータを解析してRGBAデータに変換する
        const rgbaData = convertChrRomToRgba(chrRomData, canvas.width, canvas.height);

        // ImageDataオブジェクトの作成
        const imageData = new ImageData(rgbaData, canvas.width, canvas.height);

        // Canvasに画像を描画
        ctx.putImageData(imageData, 0, 0);
    }



  return (
      <div className="container">
          <h1>Tauri NES</h1>
          {/* CPUの状態を表示 */}
          {cpuState && (
              <div className="cpu-state">
                  <p>Accumulator: {cpuState.accumulator.toString(16)}</p>
                  <p>X Register: {cpuState.x_register.toString(16)}</p>
                  <p>Y Register: {cpuState.y_register.toString(16)}</p>
                  <p>Status: {cpuState.status.toString(16)}</p>
                  <p>Program Counter: {cpuState.program_counter.toString(16)}</p>
              </div>
          )}
          <button onClick={openDialog}>Click to open NES rom</button>
          <canvas id="chrCanvas"></canvas>
      </div>
  );
}

export default App;
