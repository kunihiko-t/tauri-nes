import { invoke } from "@tauri-apps/api/tauri";
import { open } from '@tauri-apps/api/dialog'
import React, { useState, useEffect, useRef, useCallback } from "react"; // Import useState and useRef
import "./App.css";

// Define FrameData type based on Rust struct
// Ensure this matches the structure returned by the backend
type FrameData = {
    pixels: number[]; // Should be Uint8ClampedArray or number[] based on backend return
    width: number;
    height: number;
};

type ControllerState = {
    a: boolean;
    b: boolean;
    select: boolean;
    start: boolean;
    up: boolean;
    down: boolean;
    left: boolean;
    right: boolean;
};

// Mapping: key code → controller button
const keyToButtonMap: Record<string, keyof ControllerState> = {
    'KeyZ': 'a',          // Z key → A button
    'KeyX': 'b',          // X key → B button
    'Enter': 'start',     // Enter → Start button
    'ShiftRight': 'select', // Right Shift → Select button
    'ArrowUp': 'up',      // Up arrow → Up
    'ArrowDown': 'down',  // Down arrow → Down
    'ArrowLeft': 'left',  // Left arrow → Left
    'ArrowRight': 'right' // Right arrow → Right
};

// Actual NES color palette
const nesColorPalette = [
  [124, 124, 124], [0, 0, 252], [0, 0, 188], [68, 40, 188], [148, 0, 132], [168, 0, 32], [168, 16, 0], [136, 20, 0],
  [80, 48, 0], [0, 120, 0], [0, 104, 0], [0, 88, 0], [0, 64, 88], [0, 0, 0], [0, 0, 0], [0, 0, 0],
  [188, 188, 188], [0, 120, 248], [0, 88, 248], [104, 68, 252], [216, 0, 204], [228, 0, 88], [248, 56, 0], [228, 92, 16],
  [172, 124, 0], [0, 184, 0], [0, 168, 0], [0, 168, 68], [0, 136, 136], [0, 0, 0], [0, 0, 0], [0, 0, 0],
  [248, 248, 248], [60, 188, 252], [104, 136, 252], [152, 120, 248], [248, 120, 248], [248, 88, 152], [248, 120, 88], [252, 160, 68],
  [248, 184, 0], [184, 248, 24], [88, 216, 84], [88, 248, 152], [0, 232, 216], [120, 120, 120], [0, 0, 0], [0, 0, 0],
  [252, 252, 252], [164, 228, 252], [184, 184, 248], [216, 184, 248], [248, 184, 248], [248, 164, 192], [240, 208, 176], [252, 224, 168],
  [248, 216, 120], [216, 248, 120], [184, 248, 184], [184, 248, 216], [0, 252, 252], [216, 216, 216], [0, 0, 0], [0, 0, 0]
];

// Props definition for emulator status display
interface EmulatorStatusProps {
    romStatus: string;
    frameCount: number;
    isRunning: boolean;
    isTestMode: boolean;
    fps: number;
    onTestModeToggle: () => void;
    onRunToggle: () => void;
}

function App() {
    const [romLoaded, setRomLoaded] = useState(false); // State to track if ROM is loaded
    const [frameCount, setFrameCount] = useState(0); // Add frame counter
    const [hasNonZeroPixels, setHasNonZeroPixels] = useState(false); // Whether there are non-zero pixels
    const canvasRef = useRef<HTMLCanvasElement>(null); // Ref for the main game screen canvas
    const animationFrameId = useRef<number>(0); // Ref to store animation frame ID
    const [testMode, setTestMode] = useState(false); // State to track test mode
    const [romStatus, setRomStatus] = useState<string>("Not Loaded"); // State to track ROM status
    const [isRunning, setIsRunning] = useState(false); // State to track if emulator is running
    const [canvasCtx, setCanvasCtx] = useState<CanvasRenderingContext2D | null>(null); // Ref for the canvas context

    const drawFrame = useCallback(async () => {
        if (!canvasRef.current || !canvasCtx) return; // Check if canvasCtx is available
        
        const canvas = canvasRef.current;
        const ctx = canvasCtx; // Use the state variable
        const width = canvas.width;
        const height = canvas.height;

        try {
            const frameDataResult = await invoke<FrameData>('get_frame');
            console.log(`Received frame: ${frameDataResult.width}x${frameDataResult.height}`);
            console.log(`Pixel data length: ${frameDataResult.pixels.length}`);
            console.log(`Expected length (RGB): ${frameDataResult.width * frameDataResult.height * 3}`);
            console.log(`First 15 pixels:`, frameDataResult.pixels.slice(0, 15));
            const frameData = frameDataResult.pixels;
            const frameWidth = frameDataResult.width;
            const frameHeight = frameDataResult.height;

            if (frameData && frameWidth > 0 && frameHeight > 0) {
                // Ensure the canvas size matches the frame data
                if (canvas.width !== frameWidth || canvas.height !== frameHeight) {
                    canvas.width = frameWidth;
                    canvas.height = frameHeight;
                }

                const imageData = ctx.createImageData(frameWidth, frameHeight);
                
                // Convert RGBA (from backend) to ImageData's format if needed
                // Assuming frameData is already in the correct Uint8ClampedArray format
                if (frameData instanceof Uint8Array || frameData instanceof Uint8ClampedArray) {
                    imageData.data.set(frameData);
                } else if (Array.isArray(frameData)) {
                    // Handle array case if backend sends plain array
                    imageData.data.set(new Uint8ClampedArray(frameData));
                } else {
                    console.error("Received invalid frame data format:", typeof frameData);
                    // Optionally draw an error message on canvas
                    ctx.fillStyle = 'red';
                    ctx.fillRect(0, 0, width, height);
                    ctx.fillStyle = 'white';
                    ctx.font = '20px Arial';
                    ctx.fillText('Invalid Data', 10, height / 2);
                    return; // Stop further processing
                }
                
                ctx.putImageData(imageData, 0, 0); // Use ctx which is guaranteed non-null here
            } else {
                // Draw placeholder or background if no valid frame data
                ctx.fillStyle = '#333';
                ctx.fillRect(0, 0, width, height);
                ctx.fillStyle = 'white';
                ctx.font = '16px Arial';
                ctx.fillText('Waiting for frame...', 10, 30);
            }
        } catch (error) {
            console.error('Error fetching or drawing frame:', error);
            // Draw error state on canvas
            ctx.fillStyle = 'black';
            ctx.fillRect(0, 0, width, height);
            ctx.fillStyle = 'red';
            ctx.font = '20px Arial';
            ctx.fillText('Drawing Error', 10, height / 2);
        }

        // Schedule the next frame if the emulator is running
        if (isRunning) {
            animationFrameId.current = requestAnimationFrame(drawFrame);
        }
    }, [canvasCtx, isRunning, drawFrame]); // Added isRunning and drawFrame to dependencies

    useEffect(() => {
        console.log("Canvas Ref Initialized:", canvasRef.current);
        if (canvasRef.current) {
            const canvas = canvasRef.current;
            const ctx = canvas.getContext('2d', { willReadFrequently: true }); // Ensure context exists
            console.log("Canvas Context Initialized:", ctx);
            
            if (ctx) {
                setCanvasCtx(ctx);
                console.log("Canvas Context set successfully");
                
                // Start the drawing loop
                console.log("Starting initial drawFrame call");
                drawFrame(); // Call drawFrame without arguments
            } else {
                console.error("Failed to get 2D context");
            }
        }
        
        // Cleanup function to cancel animation frame on component unmount
        return () => {
            if (animationFrameId.current) {
                cancelAnimationFrame(animationFrameId.current);
                console.log("Animation frame cancelled on unmount");
            }
        };
    }, [drawFrame]); // Add drawFrame as dependency

    // Function to get RGB values from palette index
    const getRgbFromPaletteIndex = (index: number): [number, number, number] => {
        // Simple NES color palette (simplified implementation)
        const palette: [number, number, number][] = [
            [0, 0, 0],        // 0: Transparent/Black
            [126, 126, 126],  // 1: Gray
            [255, 255, 255],  // 2: White
            [255, 0, 0],      // 3: Red
            [0, 255, 0],      // 4: Green
            [0, 0, 255],      // 5: Blue
            [255, 255, 0],    // 6: Yellow
            [255, 0, 255],    // 7: Magenta
            [0, 255, 255],    // 8: Cyan
            [128, 0, 0],      // 9: Dark Red
            [0, 128, 0],      // 10: Dark Green
            [0, 0, 128],      // 11: Dark Blue
            [128, 128, 0],    // 12: Dark Yellow
            [128, 0, 128],    // 13: Dark Magenta
            [0, 128, 128],    // 14: Dark Cyan
            [192, 192, 192],  // 15: Light Gray
        ];
        
        // Keep within palette range
        const safeIndex = index % palette.length;
        return palette[safeIndex];
    };

    // Global keyboard event handler
    const handleKeyEvent = (event: KeyboardEvent) => {
        const keyCode = event.code;
        const pressed = event.type === 'keydown';
        
        // Space key default scroll action prevention
        if (keyCode === 'Space' && pressed) {
            event.preventDefault();
        }
        
        // Arrow key default action prevention (page scroll)
        if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(keyCode) && pressed) {
            event.preventDefault();
        }
        
        // Game operation key default action prevention
        if (Object.keys(keyToButtonMap).includes(keyCode) && pressed) {
            event.preventDefault();
        }
        
        // Controller button mapping processing
        if (keyCode in keyToButtonMap) {
            const button = keyToButtonMap[keyCode];
            console.log(`Button ${button} ${pressed ? 'pressed' : 'released'}`);
            // Send controller state to Rust backend
            invoke('handle_key_event', { keyCode, pressed })
                .catch(console.error);
        }
        
        // Space key to toggle test mode
        if (keyCode === 'Space') {
            if (pressed) {
                console.log('Space key pressed - toggling test mode');
                // Send test mode toggle command to backend
                invoke('handle_key_event', { keyCode, pressed })
                    .then(() => setTestMode(prevMode => !prevMode))
                    .catch(console.error);
            }
        }
    };

    // Function to clear canvas
    const clearCanvas = () => {
        const canvasRefCurrent = canvasRef;
        if (!canvasRefCurrent.current) return;

        const ctx = canvasRefCurrent.current.getContext('2d');
        if (!ctx) return;

        // Get canvas width and height
        const width = canvasRefCurrent.current.width;
        const height = canvasRefCurrent.current.height;

        // Clear canvas
        ctx.clearRect(0, 0, width, height);
        
        // Fill with black
        ctx.fillStyle = '#000';
        ctx.fillRect(0, 0, width, height);
    };

    // UseEffect to perform initialization on component mount (Simplified)
    useEffect(() => {
        // Set keyboard event listener
        window.addEventListener('keydown', handleKeyEvent);
        window.addEventListener('keyup', handleKeyEvent);

        // Initialize canvas (Keep this part)
        const canvasRefCurrent = canvasRef.current;
        if (canvasRefCurrent) {
            canvasRefCurrent.width = 256;
            canvasRefCurrent.height = 240;
            
            // Get context and start draw loop if successful
            const ctx = canvasRefCurrent.getContext('2d');
            if (ctx) {
                setCanvasCtx(ctx);
                // Start the main drawing loop (which includes get_frame)
                // The backend will initially be in test mode after load
                drawFrame(); 
            } else {
                console.error("Failed to get 2D context on mount");
                clearCanvas(); // Clear if context fails
            }
        } else {
            clearCanvas(); // Clear if canvas ref fails
        }

        // Cleanup function
        return () => {
            console.log('Performing cleanup on unmount');
            window.removeEventListener('keydown', handleKeyEvent);
            window.removeEventListener('keyup', handleKeyEvent);
            if (animationFrameId.current) {
                cancelAnimationFrame(animationFrameId.current);
            }
        };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []); // Run only once on mount

    // Function to open file dialog and load ROM
    const openDialog = async () => {
        // Clear canvas before loading new ROM
        clearCanvas();
        
        try {
            // Open file dialog
            const selected = await open({
                multiple: false,
                filters: [{
                    name: "NES ROM Files",
                    extensions: ["nes"]
                }]
            });
            
            // If no file was selected
            if (!selected) {
                console.log("No file selected");
                setRomStatus("Selection canceled");
                return;
            }
            
            // Check if one file was selected
            if (Array.isArray(selected)) {
                console.error("Multiple file selection is not supported");
                setRomStatus("Error: Multiple selection");
                return;
            }
            
            // Set loading state
            setRomStatus("Loading ROM...");
            
            // Load the ROM
            await invoke('load_rom', { filePath: selected });
            console.log(`ROM loaded: ${selected}`);
            
            // Reset frame counter
            setFrameCount(0);
            
            // Update state to indicate ROM is loaded
            setRomLoaded(true);
            setRomStatus(`ROM: ${selected.split('\\').pop()}`);
            setIsRunning(true);
            
            // Draw initial frame
            try {
                // Try to get and display the first frame
                drawFrame();
                console.log("Initial ROM frame displayed");
            } catch (error) {
                console.error("Error getting initial frame:", error);
            }
        } catch (error) {
            console.error("ROM load error:", error);
            setRomStatus(`Error: ${error}`);
            setRomLoaded(false);
        }
    };

    // Emulator status display component
    const EmulatorStatus: React.FC<EmulatorStatusProps> = ({ 
        romStatus, frameCount, isRunning, isTestMode, fps,
        onTestModeToggle, onRunToggle
    }) => {
        return (
            <div className="status-container">
                <div className="status-item">
                    <span className="status-label">Status:</span>
                    <span className="status-value">{romStatus}</span>
                </div>
                
                <div className="status-item">
                    <span className="status-label">FPS:</span>
                    <span className="status-label">{fps.toFixed(1)}</span>
                </div>
                
                <div className="status-item">
                    <span className="status-label">Frame Count:</span>
                    <span className="status-value">{frameCount}</span>
                </div>
                
                <div className="status-item action-buttons">
                    <button 
                        className={`action-button ${isTestMode ? 'active' : ''}`} 
                        onClick={onTestModeToggle}>
                        Test Mode: {isTestMode ? "Enabled" : "Disabled"}
                    </button>
                    
                    <button 
                        className={`action-button ${isRunning ? 'active' : ''}`} 
                        onClick={onRunToggle}
                        disabled={!romStatus.includes("ROM:")}
                    >
                        {isRunning ? "Stop" : "Start"}
                    </button>
                </div>
            </div>
        );
    };

    // Component for the control information section
    const ControlInfo = () => (
        <div className="control-info">
            <h3>CONTROLS</h3>
            <ul>
                <li><strong>Arrow Keys</strong>: D-pad movement</li>
                <li><strong>Z</strong>: A Button</li>
                <li><strong>X</strong>: B Button</li>
                <li><strong>Enter</strong>: Start</li>
                <li><strong>Right Shift</strong>: Select</li>
                <li><strong>Space</strong>: Toggle test pattern (debug)</li>
            </ul>
        </div>
    );

    return (
        <div className="container" style={{ cursor: 'default' }}>
            <h1>Tauri NES Emulator</h1>
            
            <div className="control-panel">
                <button onClick={openDialog}>Load NES ROM</button>
            </div>

            <EmulatorStatus romStatus={romStatus} frameCount={frameCount} isRunning={isRunning} isTestMode={testMode} fps={60} onTestModeToggle={() => {
                // Toggle test mode via the key event handler mechanism (simulating space press)
                invoke('handle_key_event', { keyCode: 'Space', pressed: true })
                    .then(() => setTestMode(prevMode => !prevMode)) // Update frontend state based on backend action
                    .catch(console.error);
            }} onRunToggle={() => {
                // Simply toggle the running state, don't reload ROM
                setIsRunning(!isRunning);
                // if (!isRunning) {
                //     setFrameCount(0);
                //     openDialog(); // DO NOT call openDialog here
                // }
            }} />

            {/* Main Game Screen Canvas */}
            <div className="canvas-container">
                <canvas
                    ref={canvasRef}
                    id="game-canvas"
                    width="256"
                    height="240"
                    style={{ 
                        border: '2px solid #333',
                        imageRendering: 'pixelated',
                        width: '512px',  // Scaled 2x for display
                        height: '480px'
                    }}
                ></canvas>
            </div>
            
            {/* コントロール情報を追加 */}
            <ControlInfo />
        </div>
    );
}

export default App;
