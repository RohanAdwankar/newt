import { Kernel, ExecutionResult } from './types';

const API_URL = 'http://127.0.0.1:3000';

export class RemoteKernel implements Kernel {
    async execute(code: string, language: string, context: string[] = []): Promise<ExecutionResult> {
        try {
            const res = await fetch(`${API_URL}/exec`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    command: code,
                    language: language,
                    context: context,
                    client_type: "web"
                })
            });
            
            if (!res.ok) {
                return {
                    stdout: "",
                    stderr: `Server error: ${res.statusText}`,
                    status: 1
                };
            }

            const data = await res.json();
            return {
                stdout: data.stdout || "",
                stderr: data.stderr || "",
                status: data.status ?? 0,
                display_data: data.display_data
            };
        } catch (e: any) {
            return {
                stdout: "",
                stderr: `Connection failed: ${e.message}`,
                status: 1
            };
        }
    }
}
