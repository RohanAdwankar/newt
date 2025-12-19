/*
 * Copyright 2020 WebAssembly Community Group participants
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

self.importScripts('shared.js');

let api;
let port;
let canvas;
let ctx2d;
let inputSab;

const apiOptions = {
  async readBuffer(filename) {
    const response = await fetch(filename);
    return response.arrayBuffer();
  },

  async compileStreaming(filename) {
    // TODO: make compileStreaming work. It needs the server to use the
    // application/wasm mimetype.
    if (false && WebAssembly.compileStreaming) {
      return WebAssembly.compileStreaming(fetch(filename));
    } else {
      const response = await fetch(filename);
      return WebAssembly.compile(await response.arrayBuffer());
    }
  },

  hostWrite(s) { port.postMessage({id : 'write', data : s}); },
  hostLogWrite(s) { port.postMessage({id : 'log', data : s}); },
  hostPrompt(msg) {
      if (!inputSab) return null;
      
      // 1. Request input
      port.postMessage({id: 'input_request', msg: msg});
      
      // 2. Wait
      const int32 = new Int32Array(inputSab);
      Atomics.store(int32, 0, 1); // Set status to WAITING (1)
      Atomics.wait(int32, 0, 1);  // Wait while status is 1
      
      // 3. Read
      const len = Atomics.load(int32, 1);
      if (len < 0) return null;
      
      const strBytes = new Uint8Array(inputSab, 8, len);
      // Copy to a non-shared buffer to satisfy TextDecoder
      const copy = new Uint8Array(len);
      copy.set(strBytes);
      const str = new TextDecoder().decode(copy);
      
      Atomics.store(int32, 0, 0); // Reset to IDLE (0)
      
      return str;
  }
};

let currentApp = null;

const onAnyMessage = async event => {
  switch (event.data.id) {
  case 'constructor':
    port = event.data.data;
    port.onmessage = onAnyMessage;
    if (event.data.inputBuffer) {
        inputSab = event.data.inputBuffer;
    }
    api = new API(apiOptions);
    break;

  case 'setShowTiming':
    api.showTiming = event.data.data;
    break;

  case 'compileToAssembly': {
    const responseId = event.data.responseId;
    let output = null;
    let transferList;
    try {
      output = await api.compileToAssembly(event.data.data);
    } finally {
      port.postMessage({id : 'runAsync', responseId, data : output},
                       transferList);
    }
    break;
  }

  case 'compileTo6502': {
    const responseId = event.data.responseId;
    let output = null;
    let transferList;
    try {
      output = await api.compileTo6502(event.data.data);
    } finally {
      port.postMessage({id : 'runAsync', responseId, data : output},
                       transferList);
    }
    break;
  }

  case 'compileLinkRun':
    if (currentApp) {
      console.log('First, disallowing rAF from previous app.');
      // Stop running rAF on the previous app, if any.
      currentApp.allowRequestAnimationFrame = false;
    }
    try {
      currentApp = await api.compileLinkRun(event.data.data);
      console.log(`finished compileLinkRun. currentApp = ${currentApp}.`);
      port.postMessage({id: 'finished', success: true});
    } catch (e) {
      console.error(e);
      port.postMessage({id: 'finished', success: false, error: e.toString()});
    }
    break;

  case 'postCanvas':
    canvas = event.data.data;
    ctx2d = canvas.getContext('2d');
    break;
  }
};

self.onmessage = onAnyMessage;
