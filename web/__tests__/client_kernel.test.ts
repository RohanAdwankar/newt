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
});
