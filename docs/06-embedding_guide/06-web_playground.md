# Web Playground Integration

The `galfus-playground-web` package provides the WebAssembly (Wasm) bridge to embed the Galfus compiler and Virtual Machine (VM) directly in the browser.

It is specifically designed to power interactive code editors (like Monaco Editor or CodeMirror), guaranteeing:
- **Cooperative Asynchronous Execution:** the VM yields control back to the browser's Event Loop, preventing UI freezing.
- **Native I/O Communication:** seamless integration with the Web Streams API (`ReadableStream` and `WritableStream`).
- **Security (Kill-Switch):** the ability to cleanly abort pending executions when restarting a script, preventing memory leaks and runaway infinite loops.

---

## Architecture Flow and Lifecycle

In Javascript, the main class is `Playground`. It encapsulates:
- The **Workspace** (for virtual file management and static analysis).
- The **Web Host** (for cooperative execution and native Web providers).

### 1. Initialization (Once per page)

Always instantiate the playground when the page or your editor's interface mounts. This instance will maintain the active virtual environment state.

```javascript
import { Playground } from "./galfus_playground_web.js";

const playground = new Playground();
```

### 2. Code Insertion / Update

As the user types in the code editor, you must synchronize the files loaded in the virtual memory. The `setSource` method creates or overwrites virtual files in the internal Workspace.

```javascript
// Updating the main entry file. The default is usually "src/main.gfs"
playground.setSource("src/main.gfs", editor.getValue());
```

### 3. Validation, Compilation, and Real-time Feedback

To display error messages, diagnostics, and red squiggles in real-time, invoke `check()` after every `setSource()` or `setConfig()` update. Only a successful check allows `compile()` to generate and cache the `PackageImage` binary.

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

*Note: `setSource()` and `setConfig()` invalidate the previous check and compiled package. The required lifecycle is `setConfig`/`setSource` → `check` → `compile` → `start`. If the latest source was not successfully checked and compiled, `start()` blocks execution and returns a `CompileRequired` error.*

### 4. Execution (Clicking the "Run" button)

A script's execution is triggered by the asynchronous `start()` method. The Galfus Wasm interface supports injecting command-line arguments, environment variables, and native browser Streams.

The method will automatically pull the latest compiled package from the cache and initialize the VM in a non-blocking way.

```javascript
// Integration example using Native Web Streams (connected to xterm.js, for example):
const writeStream = new WritableStream({
  write(chunk) {
    // Decodes the bytes and writes them to the UI Terminal
    const text = new TextDecoder().decode(chunk);
    terminal.write(text);
  }
});

const readStream = new ReadableStream({
  start(controller) {
    // Connects the Terminal's keyboard input to the Galfus VM
    terminal.onData(data => {
      controller.enqueue(new TextEncoder().encode(data));
    });
  }
});

// Optional initialization parameters
const options = {
    args: ["--mode", "release"], // CLI arguments
    envs: { "GREETING": "Hello" }, // Environment variables
    stdout: writeStream,         // Native output stream
    stdin: readStream            // Native input stream
};

// Triggers the execution.
// The "await" suspends the execution of this JS block, but the page remains
// completely responsive because Rust/Wasm will perform a cooperative yield!
const exitCode = await playground.start(options);
console.log(`Script finished with code ${exitCode}`);
```

---

## Security System: The Kill-Switch

To ensure robustness in a development environment (where infinite loops like `while (true) {}` are common), Galfus implements a **native Kill-Switch**.

If you call `await playground.start()` while **another execution is still running in the background**, the Virtual Machine will detect the concurrency and force a *Graceful Shutdown* of the previous execution before starting the newly modified script.

Benefits:
1. Prevents resource leaks in the browser tab.
2. Discards "orphan" processing.
3. Ensures that your UI's "Run/Restart" button works immediately, without blocking the thread.
