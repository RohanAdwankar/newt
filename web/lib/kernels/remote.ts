import { Kernel, ExecutionResult } from './types';

const API_URL = 'http://127.0.0.1:3030';

export class RemoteKernel implements Kernel {
    async execute(code: string, language: string, context: string[] = []): Promise<ExecutionResult> {
        // Start polling for input requests
        let polling = true;
        const poll = async () => {
            if (!polling) return;
            try {
                const res = await fetch(`${API_URL}/input/status`);
                if (res.ok && polling) {
                    const data = await res.json();
                    if (data.required) {
                        const input = prompt(data.prompt || "Input required:");
                        await fetch(`${API_URL}/input/submit`, {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ value: input || "" })
                        });
                    }
                }
            } catch (e) { 
                // Ignore polling errors
            }
            if (polling) setTimeout(poll, 1000);
        };
        poll();

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
        } finally {
            polling = false;
        }
    }
}
