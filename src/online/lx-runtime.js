'use strict';
(function() {
  // just call from rust
  function host(operation, value) {
    const result = JSON.parse(__native(operation, JSON.stringify(value)));
    if (!result.ok) throw new Error(result.error);
    return result.result;
  }

  class Buffer extends Uint8Array {
    static from(value, encoding = 'utf8') {
      if (typeof value === 'string') return new Buffer(host('encode', {
        value,
        encoding
      }));
      if (value instanceof ArrayBuffer) return new Buffer(value);
      if (value?.type === 'Buffer') value = value.data;
      return new Buffer(value);
    }

    static alloc(length, fill = 0) {
      return new Buffer(length).fill(fill);
    }

    static isBuffer(value) {
      return value instanceof Buffer;
    }

    static concat(parts) {
      const result = new Buffer(parts.reduce((total, part) => total + part.length, 0));
      let offset = 0;
      for (const part of parts) {
        result.set(part, offset);
        offset += part.length;
      }
      return result;
    }

    static byteLength(value, encoding) {
      return Buffer.from(value, encoding).length;
    }

    toString(encoding = 'utf8') {
      return host('decode', {
        bytes: Array.from(this),
        encoding
      });
    }

    toJSON() {
      return {
        type: 'Buffer',
        data: Array.from(this)
      };
    }
  }

  class TextEncoder {
    encode(value) {
      return Buffer.from(String(value));
    }
  }

  class TextDecoder {
    constructor(encoding = 'utf8') {
      this.encoding = encoding;
    }

    decode(value = []) {
      return Buffer.from(value).toString(this.encoding);
    }
  }

  class URLSearchParams {
    constructor(value = '') {
      if (value instanceof URLSearchParams) this.pairs = value.pairs.map(pair => [...pair]);
      else if (Array.isArray(value)) this.pairs = value.map(pair => pair.map(String));
      else if (typeof value === 'object') this.pairs = Object.entries(value).map(([k, v]) => [k,
        String(v)
      ]);
      else this.pairs = String(value).replace(/^\?/, '').split('&').filter(Boolean).map(
      pair => {
        const split = pair.indexOf('=');
        const decode = value => decodeURIComponent(value.replace(/\+/g, ' '));
        return split < 0 ? [decode(pair), ''] : [decode(pair.slice(0, split)), decode(pair
          .slice(split + 1))];
      });
    }

    append(key, value) {
      this.pairs.push([String(key), String(value)]);
    }

    delete(key) {
      this.pairs = this.pairs.filter(pair => pair[0] !== String(key));
    }

    set(key, value) {
      this.delete(key);
      this.append(key, value);
    }

    get(key) {
      return this.pairs.find(pair => pair[0] === String(key))?.[1] ?? null;
    }

    getAll(key) {
      return this.pairs.filter(pair => pair[0] === String(key)).map(pair => pair[1]);
    }

    has(key) {
      return this.get(key) !== null;
    }

    toString() {
      return this.pairs.map(pair => pair.map(value => encodeURIComponent(value).replace(/%20/g,
        '+')).join('=')).join('&');
    }

    entries() {
        return this.pairs[Symbol.iterator]();
      }
      [Symbol.iterator]() {
        return this.entries();
      }
  }

  class URL {
    constructor(url, base) {
      Object.assign(this, host('url', {
        url: String(url),
        base: base === undefined ? null : String(base)
      }));
    }
    get search() {
      const query = this.searchParams.toString();
      return query ? '?' + query : '';
    }
    set search(value) {
      this.searchParams = new URLSearchParams(value);
    }
    get href() {
      return this.protocol + '//' + this.host + this.pathname + this.search + this.hash;
    }
    get origin() {
      return this.protocol + '//' + this.host;
    }

    toString() {
      return this.href;
    }
  }
  globalThis.Buffer = Buffer;
  globalThis.URL = URL;
  globalThis.URLSearchParams = URLSearchParams;
  globalThis.TextEncoder = TextEncoder;
  globalThis.TextDecoder = TextDecoder;
  globalThis.window = globalThis;
  globalThis.self = globalThis;
  globalThis.atob = value => Buffer.from(value, 'base64').toString('binary');
  globalThis.btoa = value => Buffer.from(value, 'binary').toString('base64');
  globalThis.console = new Proxy({}, {
    get: () => () => {}
  });
  const timers = new Map();
  let sequence = 0;
  globalThis.setTimeout = (callback, delay = 0, ...args) => {
    const id = ++sequence;
    timers.set(id, {
      callback,
      due: Date.now() + Math.max(0, Math.min(Number(delay) || 0, 60000)),
      args
    });
    return id;
  };
  globalThis.clearTimeout = id => timers.delete(id);
  globalThis.__tick = () => {
    for (const [id, timer] of timers)
      if (timer.due <= Date.now()) {
        timers.delete(id);
        timer.callback(...timer.args);
      }
  };
  const requests = new Map();
  globalThis.__deliver = (id, packet) => {
    const callback = requests.get(id);
    if (!callback) return;
    requests.delete(id);
    if (!packet.ok) callback(new Error(packet.error));
    else {
      const response = packet.result;
      response.raw = Buffer.from(response.raw);
      callback(null, response, response.body);
    }
  };
  globalThis.__settled = null;
  globalThis.__handler = null;
  globalThis.lx = {
    EVENT_NAMES: {
      request: 'request',
      inited: 'inited',
      updateAlert: 'updateAlert'
    },
    env: 'desktop',
    version: '2.0.0',
    currentScriptInfo: {
      rawScript: __rawScript,
      name: 'MIG 内置源',
      version: '1.0.0'
    },
    request(url, options = {}, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      const id = ++sequence;
      requests.set(id, callback);
      const normalized = {
        ...options
      };
      if (normalized.body instanceof Uint8Array) normalized.body = Array.from(normalized.body);
      __request(id, String(url), JSON.stringify(normalized));
      return () => {
        __cancel(id);
        __deliver(id, {
          ok: false,
          error: '请求已取消'
        });
      };
    },
    on(event, callback) {
      if (event !== 'request') return Promise.reject(new Error('不支持的事件'));
      __handler = callback;
      return Promise.resolve();
    },
    send(event, info) {
      if (event === 'inited') __settled = {
        ok: true,
        result: info
      };
      return Promise.resolve();
    },
    utils: {
      buffer: {
        from: (...args) => Buffer.from(...args),
        bufToString: (value, format) => Buffer.from(value).toString(format)
      },
      crypto: {
        md5: value => host('md5', Array.from(Buffer.from(value))),
        randomBytes: length => Buffer.from(host('random', length)),
        aesEncrypt: (value, mode, key, iv) => Buffer.from(host('aes', {
          bytes: Array.from(Buffer.from(value)),
          mode,
          key: Array.from(Buffer.from(key)),
          iv: Array.from(Buffer.from(iv || [])),
        })),
        rsaEncrypt: (value, key) => Buffer.from(host('rsa', {
          bytes: Array.from(Buffer.from(value)),
          key
        })),
      },
      zlib: {
        inflate: async value => Buffer.from(host('inflate', Array.from(Buffer.from(value)))),
        deflate: async value => Buffer.from(host('deflate', Array.from(Buffer.from(value)))),
      },
    },
  };
  globalThis.__invoke = value => {
    __settled = null;
    Promise.resolve().then(() => __handler(value)).then(
      result => {
        __settled = {
          ok: true,
          result
        };
      },
      error => {
        __settled = {
          ok: false,
          error: String(error?.message || error)
        };
      },
    );
  };
})();
