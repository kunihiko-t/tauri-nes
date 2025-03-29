import { invoke } from "@tauri-apps/api/tauri";
import { open } from '@tauri-apps/api/dialog'
import React, { useState, useEffect, useRef } from "react"; // Import useState and useRef
import "./App.css";

// Define FrameData type based on Rust struct
// Ensure this matches the structure returned by the backend
type FrameData = {
    pixels: number[]; // Should be Uint8ClampedArray or number[] based on backend return
    width: number;
    height: number;
};

function App() {
    const [romLoaded, setRomLoaded] = useState(false); // State to track if ROM is loaded
    const canvasRef = useRef<HTMLCanvasElement>(null); // Ref for the main game screen canvas
    const animationFrameId = useRef<number | null>(null); // Ref to store animation frame ID

    // キーボードイベントを処理し、Rustのバックエンドに送信する
    useEffect(() => {
        // Define ControllerState inside useEffect or import from a types file
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
                // Use invoke with the correct structure for inputData
                invoke('handle_input', { inputData: { ...controllerState } }); // Send a copy
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

    // Function to draw frame data onto the main canvas
    const drawFrame = (frameData: FrameData) => {
        const canvas = canvasRef.current;
        if (!canvas || !frameData) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        const { pixels, width, height } = frameData;
        // Canvas dimensions should match the NES resolution
        canvas.width = width;
        canvas.height = height;

        // Create ImageData from the raw pixel data (RGBA)
        // Use Uint8ClampedArray for ImageData constructor
        const imageData = new ImageData(new Uint8ClampedArray(pixels), width, height);

        // Draw the ImageData onto the canvas
        ctx.putImageData(imageData, 0, 0);
    };

    // Main emulation loop using requestAnimationFrame
    const runEmulatorLoop = async () => {
        // Only run the loop logic if a ROM is loaded
        if (romLoaded) {
            try {
                // Fetch the frame data from the backend
                const frameData: FrameData = await invoke('run_emulator_frame');
                // Draw the received frame onto the canvas
                drawFrame(frameData);
            } catch (error) {
                console.error('Error running emulator frame:', error);
                // Consider stopping the loop or showing an error message
                setRomLoaded(false); // Stop the loop if backend error occurs
            }
        }

        // Request the next animation frame to continue the loop
        animationFrameId.current = requestAnimationFrame(runEmulatorLoop);
    };

    // Effect to start and stop the emulation loop
    useEffect(() => {
        // Start the loop when the component mounts
        animationFrameId.current = requestAnimationFrame(runEmulatorLoop);

        // Cleanup function to cancel the animation frame when the component unmounts
        return () => {
            if (animationFrameId.current) {
                cancelAnimationFrame(animationFrameId.current);
            }
        };
    }, [romLoaded]); // Dependency array includes romLoaded to restart loop logic if it changes

    // Removed convertChrRomToRgba function
    /*
    function convertChrRomToRgba(...) { ... }
    */

    async function openDialog() {
        const selected = await open({
            multiple: false,
            filters: [{ name: 'NES ROM', extensions: ['nes'] }]
        });

        if (typeof selected === 'string') { // Check if a file was selected
            const filePath = selected;
            console.log("Selected file path:", filePath);
            try {
                // Call the correct command to load the ROM
                await invoke('load_rom', { filePath });
                console.log('ROM loaded successfully via dialog');
                setRomLoaded(true); // Update state to indicate ROM is loaded
            } catch (error) {
                console.error('Error invoking load_rom:', error);
                setRomLoaded(false); // Update state on error
                // TODO: Display error message to the user
            }
        } else {
            console.log("File selection cancelled.");
            setRomLoaded(false); // Ensure state reflects no ROM loaded
        }
    }

    // Removed drawChrRom function
    /*
    function drawChrRom(...) { ... }
    */

  return (
      <div className="container">
          <h1>Tauri NES</h1>
          <button onClick={openDialog}>Load NES ROM</button>

          {/* Main Game Screen Canvas */}
          <div className="emulator-screen" style={{ marginTop: '10px' }}>
                <canvas
                    ref={canvasRef}
                    style={{
                        border: '1px solid black',
                        imageRendering: 'pixelated', // Keep pixels sharp when scaled
                        width: '512px', // Scale canvas for display (2x width)
                        height: '480px' // Scale canvas for display (2x height)
                    }}
                ></canvas>
                {!romLoaded && <p style={{ textAlign: 'center', marginTop: '5px' }}>Please load a .nes ROM file.</p>}
            </div>

          {/* Removed CHR ROM Canvas */}
          {/* <canvas id="chrCanvas" ... ></canvas> */}
      </div>
  );
}

export default App;
