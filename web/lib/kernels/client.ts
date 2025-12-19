import { Kernel, ExecutionResult } from './types';

// Global Pyodide instance
let pyodide: any = null;
let pyodideLoadingPromise: Promise<any> | null = null;

async function loadPyodide() {
    if (pyodide) return pyodide;
    if (pyodideLoadingPromise) return pyodideLoadingPromise;

    pyodideLoadingPromise = new Promise((resolve, reject) => {
        // Check if script is already loaded
        if ((window as any).loadPyodide) {
             // @ts-ignore
             (window as any).loadPyodide().then(p => {
                 pyodide = p;
                 resolve(p);
             }).catch(reject);
             return;
        }

        const script = document.createElement('script');
        script.src = "https://cdn.jsdelivr.net/pyodide/v0.25.0/full/pyodide.js";
        script.onload = async () => {
            try {
                // @ts-ignore
                const p = await (window as any).loadPyodide();
                pyodide = p;
                resolve(p);
            } catch (e) {
                reject(e);
            }
        };
        script.onerror = (e) => reject(e);
        document.body.appendChild(script);
    });

    return pyodideLoadingPromise;
}

export class ClientKernel implements Kernel {
    private context: any = {};

    private clangPort: MessagePort | null = null;
    private clangInitPromise: Promise<void> | null = null;

    private async initClang() {
        if (this.clangPort) return;
        if (this.clangInitPromise) return this.clangInitPromise;

        this.clangInitPromise = new Promise((resolve, reject) => {
            if (typeof Worker === 'undefined') {
                reject(new Error("Web Workers not supported"));
                return;
            }

            // Helper to determine base path from Next.js scripts
            const getBasePath = () => {
                if (typeof window === 'undefined') return '';
                const scripts = document.getElementsByTagName('script');
                for (let i = 0; i < scripts.length; i++) {
                    const src = scripts[i].src;
                    if (src && src.includes('/_next/')) {
                        try {
                            const url = new URL(src);
                            const path = url.pathname;
                            const nextIndex = path.indexOf('/_next/');
                            if (nextIndex !== -1) {
                                return path.substring(0, nextIndex);
                            }
                        } catch (e) {
                            console.warn("Failed to parse script URL:", src);
                        }
                    }
                }
                return '';
            };

            const basePath = getBasePath();
            const workerUrl = `${basePath}/wasm-clang/worker.js`;
            console.log("Initializing Clang worker from:", workerUrl);

            const worker = new Worker(workerUrl);
            
            // Handle worker loading errors
            worker.onerror = (e) => {
                console.error("Clang worker error:", e);
                reject(new Error(`Failed to load Clang worker from ${workerUrl}`));
            };

            const channel = new MessageChannel();
            this.clangPort = channel.port1;
            this.clangPort.start();

            // Setup SharedArrayBuffer for synchronous input
            const sab = new SharedArrayBuffer(1024);
            const int32 = new Int32Array(sab);

            this.clangPort.addEventListener('message', (e) => {
                const msg = e.data;
                if (msg.id === 'input_request') {
                    const input = prompt(msg.msg || "Input required:");
                    if (input === null) {
                        Atomics.store(int32, 1, -1);
                    } else {
                        const encoder = new TextEncoder();
                        const bytes = encoder.encode(input);
                        const uint8 = new Uint8Array(sab, 8);
                        const len = Math.min(bytes.length, uint8.length);
                        uint8.set(bytes.subarray(0, len));
                        Atomics.store(int32, 1, len);
                    }
                    Atomics.store(int32, 0, 0); // Reset status to 0 (IDLE) - wait, worker sets it to 0.
                    // Worker waits for 1. We should set it to something else?
                    // Worker: store(0, 1), wait(0, 1).
                    // Main: store(0, 0), notify(0).
                    // Worker wakes up, sees 0.
                    
                    Atomics.store(int32, 0, 0);
                    Atomics.notify(int32, 0);
                }
            });
            
            // Add a timeout for initialization
            const timeout = setTimeout(() => {
                reject(new Error("Clang worker initialization timed out"));
            }, 10000);

            // We assume it's ready if we don't get an error immediately, 
            // but ideally the worker should send a 'ready' message.
            // For now, we just resolve.
            worker.postMessage({id: 'constructor', data: channel.port2, inputBuffer: sab}, [channel.port2]);
            
            clearTimeout(timeout);
            resolve();
        });
        return this.clangInitPromise;
    }

    private async executeC(code: string, language: string): Promise<ExecutionResult> {
        try {
            await this.initClang();
            if (!this.clangPort) throw new Error("Clang worker not initialized");

            return new Promise((resolve) => {
                let stdout = "";
                
                const handler = (e: MessageEvent) => {
                    const msg = e.data;
                    if (msg.id === 'write') {
                        stdout += msg.data;
                    } else if (msg.id === 'log') {
                        // Ignore build logs
                        // console.log("Build log:", msg.data);
                    } else if (msg.id === 'finished') {
                        this.clangPort!.removeEventListener('message', handler);
                        resolve({
                            stdout,
                            stderr: msg.success ? "" : (msg.error || "Unknown error"),
                            status: msg.success ? 0 : 1
                        });
                    }
                };

                this.clangPort!.addEventListener('message', handler);

                // Check if code has main function
                let codeToRun = code;
                if (!code.match(/(int|void)\s+main\s*\(/)) {
                    // Wrap in main
                    if (language === 'cpp') {
                        codeToRun = `#include <iostream>
#include <string>
#include <vector>
#include <algorithm>
#include <cmath>
#include <cstdio>

int main() {
${code}
    std::cout << std::flush;
    return 0;
}`;
                    } else {
                        codeToRun = `#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

int main() {
${code}
    fflush(stdout);
    return 0;
}`;
                    }
                }

                this.clangPort!.postMessage({
                    id: 'compileLinkRun',
                    data: codeToRun
                });
            });
        } catch (e: any) {
            return {
                stdout: "",
                stderr: e.toString(),
                status: 1
            };
        }
    }

    async execute(code: string, language: string, _context: string[] = []): Promise<ExecutionResult> {
        if (language === 'javascript' || language === 'typescript') {
            return this.executeJS(code);
        } else if (language === 'python') {
            return this.executePython(code);
        } else if (language === 'c' || language === 'cpp') {
            return this.executeC(code, language);
        } else if (language === 'shell') {
            // Treat shell as python for now if it looks like python? No, that's confusing.
            // But if the default cell type is shell, and user types python...
            // The UI handles "magic commands" to switch type.
            // If we are here, it means the cell type is explicitly 'shell'.
            return {
                stdout: "",
                stderr: "Client-side shell execution is not supported. Use !python or switch cell type.",
                status: 1
            };
        } else {
            // TODO: Implement WASM support for compiled languages (Rust, C, Go)
            // See wasm-clang for reference
            return {
                stdout: "",
                stderr: `Client-side execution not supported for ${language}`,
                status: 1
            };
        }
    }

    private async executeJS(code: string): Promise<ExecutionResult> {
        const logs: string[] = [];
        
        let originalConsoleLog;
        let originalConsoleError;
        
        // Only patch global console if in browser and NOT using VM
        // We'll determine VM usage below
        
        let vm;
        try {
            // Try to load vm module if in Node environment (including JSDOM)
            // We use eval('require') to prevent Webpack from bundling 'vm' for the browser
            if (typeof process !== 'undefined' && process.versions && process.versions.node) {
                vm = eval('require')('vm');
            }
        } catch (e) {
            // Ignore error if vm cannot be loaded
        }

        if (!vm && typeof window !== 'undefined') {
            originalConsoleLog = console.log;
            originalConsoleError = console.error;
            console.log = (...args) => logs.push(args.join(' '));
            console.error = (...args) => logs.push(args.join(' '));
        }

        try {
            let result;
            if (vm) {
                // Node environment (Jest/Server) - Use vm for persistence
                
                // Initialize context if needed
                if (!this.context.vmContext) {
                    this.context.vmContext = vm.createContext({
                        console: {
                            log: () => {},
                            error: () => {},
                            warn: console.warn,
                            info: console.info
                        }
                    });
                }
                
                // Update console methods to capture logs for THIS execution
                this.context.vmContext.console.log = (...args: any[]) => { logs.push(args.join(' ')); };
                this.context.vmContext.console.error = (...args: any[]) => { logs.push(args.join(' ')); };
                
                // Run code in persistent context
                result = vm.runInContext(code, this.context.vmContext);
            } else {
                // Browser environment - Use eval
                // ...
                result = (window as any).eval(code);
            }

            
            return {
                stdout: logs.join('\n') + (result !== undefined ? '\n' + String(result) : ''),
                stderr: "",
                status: 0
            };
        } catch (e: any) {
            return {
                stdout: logs.join('\n'),
                stderr: e.toString(),
                status: 1
            };
        } finally {
            if (originalConsoleLog) console.log = originalConsoleLog;
            if (originalConsoleError) console.error = originalConsoleError;
        }
    }

    private async executePython(code: string): Promise<ExecutionResult> {
        try {
            const p = await loadPyodide();
            
            // Capture stdout
            p.setStdout({ batched: (msg: string) => {} }); // Reset
            let stdout = "";
            p.setStdout({ batched: (msg: string) => { stdout += msg + "\n"; } });
            
            // Handle stdin
            p.setStdin({ stdin: () => prompt("Input required:") || "" });
            
            await p.loadPackagesFromImports(code);
            const result = await p.runPythonAsync(code);
            
            return {
                stdout: stdout + (result !== undefined ? String(result) : ""),
                stderr: "",
                status: 0
            };
        } catch (e: any) {
            return {
                stdout: "",
                stderr: e.toString(),
                status: 1
            };
        }
    }
}
