import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import "./App.css";

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");
  const [romPath, setRomPath] = useState("");
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRequestIdRef = useRef<number | null>(null);

  // キー入力イベントのハンドラー
  const handleKeyDown = (event: KeyboardEvent) => {
    // スペースキーなどのキー入力をエミュレータに送信
    invoke("handle_keyboard_event", { keyCode: event.code, pressed: true })
      .catch(console.error);
  };

  const handleKeyUp = (event: KeyboardEvent) => {
    invoke("handle_keyboard_event", { keyCode: event.code, pressed: false })
      .catch(console.error);
  };

  useEffect(() => {
    // キーボードイベントリスナーを追加
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);

    // クリーンアップ時にイベントリスナーを削除
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, []);

  useEffect(() => {
    if (!canvasRef.current) return;

    const runFrame = async () => {
      try {
        const frameData = await invoke("run_emulator_frame");
        
        if (frameData && canvasRef.current) {
          const canvas = canvasRef.current;
          const ctx = canvas.getContext("2d");
          
          if (ctx && frameData.width && frameData.height) {
            const imageData = ctx.createImageData(frameData.width, frameData.height);
            
            // Convert frameData.pixels to Uint8ClampedArray for ImageData
            const pixelsArray = new Uint8ClampedArray(frameData.pixels);
            imageData.data.set(pixelsArray);
            
            // Draw the image data to the canvas
            ctx.putImageData(imageData, 0, 0);
          }
        }
        
        // Request the next frame
        frameRequestIdRef.current = requestAnimationFrame(runFrame);
      } catch (error) {
        console.error("Error running frame:", error);
      }
    };

    // Start the animation loop
    frameRequestIdRef.current = requestAnimationFrame(runFrame);

    // Clean up on unmount
    return () => {
      if (frameRequestIdRef.current !== null) {
        cancelAnimationFrame(frameRequestIdRef.current);
      }
    };
  }, [canvasRef]);

  async function loadROM() {
    try {
      await invoke("load_rom", { filePath: romPath });
      console.log("ROM loaded successfully");
    } catch (error) {
      console.error("Failed to load ROM:", error);
    }
  }

  return (
    <div className="container">
      <h1>NES Emulator</h1>

      <div className="row">
        <div>
          <input
            id="rom-path-input"
            onChange={(e) => setRomPath(e.currentTarget.value)}
            placeholder="Enter ROM path"
            value={romPath}
          />
          <button type="button" onClick={loadROM}>
            Load ROM
          </button>
        </div>
      </div>

      <div className="row">
        <canvas 
          ref={canvasRef} 
          width="256" 
          height="240"
          style={{ 
            border: "1px solid #ccc", 
            imageRendering: "pixelated",
            width: "512px",
            height: "480px"
          }}
        />
      </div>

      <p className="controls-info">
        コントロール: スペースキー = テストモード切替
      </p>
    </div>
  );
}

export default App; 