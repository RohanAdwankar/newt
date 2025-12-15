import { Kernel, ExecutionMode } from './types';
import { RemoteKernel } from './remote';
import { ClientKernel } from './client';

export * from './types';

export function createKernel(mode: ExecutionMode): Kernel {
    switch (mode) {
        case 'client':
            return new ClientKernel();
        case 'remote':
        default:
            return new RemoteKernel();
    }
}
