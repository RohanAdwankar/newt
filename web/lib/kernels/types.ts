export interface ExecutionResult {
    stdout: string;
    stderr: string;
    status: number;
    display_data?: any;
}

export interface Kernel {
    execute(code: string, language: string, context?: string): Promise<ExecutionResult>;
}

export type ExecutionMode = 'client' | 'remote';
