import { ClientKernel } from '../lib/kernels/client';

describe('Client Kernel Tests', () => {
    const clientKernel = new ClientKernel();

    describe('JavaScript Execution', () => {
        const code = "console.log('hello'); 1 + 1;";
        const expectedOutput = "hello\n2";

        test('Executes JS correctly', async () => {
            const result = await clientKernel.execute(code, 'javascript');
            expect(result.status).toBe(0);
            expect(result.stdout.trim()).toBe(expectedOutput);
        });
    });

    describe('TypeScript Execution', () => {
        const code = "const x = 10; x * 2;";
        const expectedOutput = "20";

        test('Executes TS correctly', async () => {
            const result = await clientKernel.execute(code, 'typescript');
            expect(result.status).toBe(0);
            expect(result.stdout.trim()).toBe(expectedOutput);
        });
    });

    describe('Execution Order & State Persistence', () => {
        test('Respects Execution Order', async () => {
            const kernel = new ClientKernel();
            
            // 1. Run Cell 1
            await kernel.execute("var x = 10;", "javascript");
            
            // 2. Run Cell 2
            await kernel.execute("var y = 13;", "javascript");
            
            // 3. Run Modified Cell 1
            // In a persistent kernel, 'y' is available from step 2.
            await kernel.execute("x = 10 + y;", "javascript");
            
            // 4. Run Cell 3
            const result = await kernel.execute("console.log(x + y);", "javascript");
            
            expect(result.status).toBe(0);
            // x is now 23 (10 + 13). y is 13. x + y = 36.
            expect(result.stdout.trim()).toBe("36");
        });
    });

    // Note: Python and C++ tests are skipped in Jest environment because they require
    // browser APIs (Web Workers, script tags) that are not fully mocked here.
    // They should be tested in E2E tests.

    describe('Input Support', () => {
        test('JS Input works', async () => {
            const kernel = new ClientKernel();
            
            // Mock prompt
            const originalPrompt = window.prompt;
            window.prompt = jest.fn().mockReturnValue("HelloJS");
            
            const code = "const x = prompt('Enter: '); console.log(`Got: ${x}`);";
            const result = await kernel.execute(code, 'javascript');
            
            expect(result.status).toBe(0);
            expect(result.stdout.trim()).toBe("Got: HelloJS");
            expect(window.prompt).toHaveBeenCalledWith("Enter: ");
            
            window.prompt = originalPrompt;
        });

        test('Python Input works (Mocked)', async () => {
            const kernel = new ClientKernel();
            
            // Mock prompt
            const originalPrompt = window.prompt;
            window.prompt = jest.fn().mockReturnValue("HelloPython");

            // Mock Pyodide
            const mockPyodide = {
                setStdout: jest.fn(),
                setStdin: jest.fn(),
                loadPackagesFromImports: jest.fn(),
                runPythonAsync: jest.fn().mockImplementation(async (code) => {
                    return "Got: HelloPython";
                })
            };
            (window as any).loadPyodide = jest.fn().mockResolvedValue(mockPyodide);

            const code = "x = input('Enter: '); print(f'Got: {x}')";
            const result = await kernel.execute(code, 'python');
            
            expect(result.status).toBe(0);
            expect(result.stdout).toContain("Got: HelloPython");
            
            // Verify setStdin was called
            expect(mockPyodide.setStdin).toHaveBeenCalled();
            const stdinHandler = mockPyodide.setStdin.mock.calls[0][0].stdin;
            stdinHandler();
            expect(window.prompt).toHaveBeenCalledWith("Input required:");
            
            window.prompt = originalPrompt;
        });

        test('C++ Input works (Mocked)', async () => {
            const kernel = new ClientKernel();
            
            // Mock prompt
            const originalPrompt = window.prompt;
            window.prompt = jest.fn().mockReturnValue("HelloCpp");

            // Mock Worker
            class MockWorker {
                onerror: any;
                onmessage: any;
                postMessage(msg: any) {}
            }
            (window as any).Worker = MockWorker;
            (window as any).SharedArrayBuffer = ArrayBuffer; // Mock SAB
            (window as any).Atomics = {
                store: jest.fn(),
                notify: jest.fn(),
                wait: jest.fn(),
                load: jest.fn()
            };

            // Mock MessageChannel
            const listeners: any[] = [];
            const port1 = {
                start: jest.fn(),
                addEventListener: jest.fn((event, cb) => {
                    listeners.push(cb);
                }),
                removeEventListener: jest.fn(),
                postMessage: jest.fn((msg) => {
                    if (msg.id === 'compileLinkRun') {
                        // Simulate input request
                        listeners.forEach(cb => cb({ data: { id: 'input_request', msg: 'Enter: ' } }));
                        
                        // Simulate output after input
                        listeners.forEach(cb => cb({ data: { id: 'write', data: 'Got: HelloCpp' } }));
                        listeners.forEach(cb => cb({ data: { id: 'finished', success: true } }));
                    }
                })
            };
            const port2 = {};
            (window as any).MessageChannel = jest.fn().mockReturnValue({ port1, port2 });

            const code = "std::string x; std::cin >> x; std::cout << \"Got: \" << x;";
            const result = await kernel.execute(code, 'cpp');
            
            expect(result.status).toBe(0);
            expect(result.stdout).toContain("Got: HelloCpp");
            expect(window.prompt).toHaveBeenCalledWith("Enter: ");
            
            window.prompt = originalPrompt;
        });
    });
});
