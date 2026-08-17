# Galfus Playground Web

This package provides the WebAssembly (Wasm) bridge to embed the Galfus compiler and Virtual Machine (VM) directly in the browser.

It is designed to create interactive code editors (like Monaco Editor or CodeMirror), guaranteeing secure, asynchronous execution (does not freeze the browser tab) and support for I/O via native Web Streams.

## Architecture Flow and Lifecycle

In Javascript, you will have access to the `Playground` class. It encapsulates the `galfus-workspace`, which manages the virtual storage of source files, and the `galfus-host-web`, which provides cooperative execution in the VM (non-blocking Event Loop).

### 1. Initialization (Once per page)

Always instantiate the playground when the page or the editor's UI mounts. It will initialize the virtual environment.

```javascript
import { Playground } from "./galfus_playground_web.js";

const playground = new Playground();
```

### 2. Code Insertion / Update

As the user types in the code editor, you must update the files loaded in the virtual memory. The `setSource` method overwrites any previous content if the file already exists.

```javascript
// The name of the virtual file. The default is usually "src/main.gfs"
playground.setSource("src/main.gfs", editor.getValue());
```

### 3. Validation and Compilation (Real Time)

To display error messages and red squiggles in real time, call `check()` after every `setSource()` or `setConfig()` update. A successful check is required before `compile()` can generate and cache the binary.

```javascript
const checkResult = JSON.parse(playground.check());

if (!checkResult.is_valid) {
    console.error("Validation errors:", checkResult.diagnostics);
    // Render the formatted diagnostics in the editor.
    return;
}

const compResult = JSON.parse(playground.compile());

if (!compResult.ok) {
    console.error("Build error:", compResult.error);
}
```

*Note: `setSource()` and `setConfig()` invalidate the previous check and compiled package. The required lifecycle is `setConfig`/`setSource` → `check` → `compile` → `start`. If the latest source was not successfully checked and compiled, `start()` blocks execution.*

### 4. Execution (Clicking the "Run" button)

Script execution is triggered by the asynchronous `start()` method. The Galfus interface supports injecting arguments, environment variables, and standard terminal Streams using native Browser APIs (`ReadableStream` for input and `WritableStream` for output).

The `start` method will pull the last compiled package from the cache and initialize the Galfus Virtual Machine in a non-blocking manner.

```javascript
// Integration example using Native Web Streams:
const writeStream = new WritableStream({
  write(chunk) {
    // Writes the bytes to your Terminal (Xterm.js, etc)
    const text = new TextDecoder().decode(chunk);
    terminal.write(text);
  }
});

const readStream = new ReadableStream({
  start(controller) {
    // Connects your terminal's keyboard to the VM
    terminal.onData(data => {
      controller.enqueue(new TextEncoder().encode(data));
    });
  }
});

// Optional parameters
const options = {
    args: ["--mode", "release"], // command args
    envs: { "GREETING": "Hello" }, // Environment variables
    stdout: writeStream,         // Output stream
    stdin: readStream            // Input stream
};

// Wait for execution to finish.
// Thanks to WASM cooperative yielding, the browser will *not* freeze on this promise!
const exitCode = await playground.start(options);
console.log(`Script finished with code ${exitCode}`);
```

## Security System (Kill-Switch)

The `Playground` features a smart execution state control system. 
If you call the `await playground.start()` function while **another execution is still running in the background**, the Virtual Machine detects the new request, safely aborts (*Graceful Shutdown*) its previous execution, and initializes the new script immediately.

This prevents:
1. Memory leaks.
2. Accumulation of concurrent processing in the same tab.
3. Freezes in infinite logic like `while (true) {}`. You can simply restart by pressing "Run" again.
