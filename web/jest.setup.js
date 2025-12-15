import '@testing-library/jest-dom'
import { TextEncoder, TextDecoder } from 'util'

global.TextEncoder = TextEncoder
global.TextDecoder = TextDecoder

if (!global.Request) {
    global.Request = class Request {
        constructor(input, init) {
            this.url = input;
            this.init = init || {};
            this.headers = new Headers(this.init.headers);
        }
        async json() { return JSON.parse(this.init.body); }
    };
}

if (!global.Response) {
    global.Response = class Response {
        constructor(body, init) {
            this.body = body;
            this.init = init || {};
            this.status = this.init.status || 200;
        }
        async json() { return typeof this.body === 'string' ? JSON.parse(this.body) : this.body; }
    };
}

if (!global.Headers) {
    global.Headers = class Headers {
        constructor(init) {
            this.map = new Map(Object.entries(init || {}));
        }
        get(key) { return this.map.get(key); }
    };
}
