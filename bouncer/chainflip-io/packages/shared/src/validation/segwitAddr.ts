// Copyright (c) 2017, 2021 Pieter Wuille
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

/* eslint-disable no-bitwise */

const CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
const GENERATOR = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

enum Encoding {
  Bech32 = 'bech32',
  Bech32m = 'bech32m',
}

const bech32 = {
  getEncodingConst(enc: Encoding) {
    if (enc === Encoding.Bech32) return 1;
    if (enc === Encoding.Bech32m) return 0x2bc830a3;
    return null;
  },
  polymod(values: number[]) {
    let chk = 1;
    for (let p = 0; p < values.length; p += 1) {
      const top = chk >> 25;
      chk = ((chk & 0x1ffffff) << 5) ^ values[p];
      for (let i = 0; i < 5; i += 1) {
        if ((top >> i) & 1) {
          chk ^= GENERATOR[i];
        }
      }
    }
    return chk;
  },
  hrpExpand(hrp: string) {
    const ret = [];
    let p;
    for (p = 0; p < hrp.length; p += 1) {
      ret.push(hrp.charCodeAt(p) >> 5);
    }
    ret.push(0);
    for (p = 0; p < hrp.length; p += 1) {
      ret.push(hrp.charCodeAt(p) & 31);
    }
    return ret;
  },
  verifyChecksum(hrp: string, data: number[], enc: Encoding) {
    return (
      this.polymod(this.hrpExpand(hrp).concat(data)) ===
      this.getEncodingConst(enc)
    );
  },

  createChecksum(hrp: string, data: number[], enc: Encoding) {
    const values = this.hrpExpand(hrp).concat(data).concat([0, 0, 0, 0, 0, 0]);
    // @ts-expect-error can be null
    const mod = this.polymod(values) ^ this.getEncodingConst(enc);
    const ret = [];
    for (let p = 0; p < 6; p += 1) {
      ret.push((mod >> (5 * (5 - p))) & 31);
    }
    return ret;
  },

  encode(hrp: string, data: number[], enc: Encoding) {
    const combined = data.concat(this.createChecksum(hrp, data, enc));
    let ret = `${hrp}1`;
    for (let p = 0; p < combined.length; p += 1) {
      ret += CHARSET.charAt(combined[p]);
    }
    return ret;
  },

  decode(bechString: string, enc: Encoding) {
    let p: number;
    let hasLower = false;
    let hasUpper = false;
    for (p = 0; p < bechString.length; p += 1) {
      if (bechString.charCodeAt(p) < 33 || bechString.charCodeAt(p) > 126) {
        return null;
      }
      if (bechString.charCodeAt(p) >= 97 && bechString.charCodeAt(p) <= 122) {
        hasLower = true;
      }
      if (bechString.charCodeAt(p) >= 65 && bechString.charCodeAt(p) <= 90) {
        hasUpper = true;
      }
    }
    if (hasLower && hasUpper) return null;
    const lowercase = bechString.toLowerCase();
    const pos = lowercase.lastIndexOf('1');
    if (pos < 1 || pos + 7 > lowercase.length || lowercase.length > 90) {
      return null;
    }
    const hrp = lowercase.substring(0, pos);
    const data = [];
    for (p = pos + 1; p < lowercase.length; p += 1) {
      const d = CHARSET.indexOf(lowercase.charAt(p));
      if (d === -1) return null;
      data.push(d);
    }
    if (!this.verifyChecksum(hrp, data, enc)) {
      return null;
    }

    return { hrp, data: data.slice(0, data.length - 6) };
  },
};

export const segwitAddress = {
  convertbits(data: number[], frombits: number, tobits: number, pad: boolean) {
    let acc = 0;
    let bits = 0;
    const ret = [];
    const maxv = (1 << tobits) - 1;
    for (let p = 0; p < data.length; p += 1) {
      const value = data[p];
      if (value < 0 || value >> frombits !== 0) {
        return null;
      }
      acc = (acc << frombits) | value;
      bits += frombits;
      while (bits >= tobits) {
        bits -= tobits;
        ret.push((acc >> bits) & maxv);
      }
    }
    if (pad) {
      if (bits > 0) {
        ret.push((acc << (tobits - bits)) & maxv);
      }
    } else if (bits >= frombits || (acc << (tobits - bits)) & maxv) {
      return null;
    }
    return ret;
  },

  decode(hrp: string, addr: string) {
    let bech32m = false;
    let dec = bech32.decode(addr, Encoding.Bech32);
    if (dec === null) {
      dec = bech32.decode(addr, Encoding.Bech32m);
      bech32m = true;
    }
    if (
      dec === null ||
      dec.hrp !== hrp ||
      dec.data.length < 1 ||
      dec.data[0] > 16
    ) {
      return null;
    }
    const res = this.convertbits(dec.data.slice(1), 5, 8, false);
    if (res === null || res.length < 2 || res.length > 40) {
      return null;
    }
    if (dec.data[0] === 0 && res.length !== 20 && res.length !== 32) {
      return null;
    }
    if (dec.data[0] === 0 && bech32m) {
      return null;
    }
    if (dec.data[0] !== 0 && !bech32m) {
      return null;
    }
    return { version: dec.data[0], program: res };
  },

  encode(hrp: string, version: number, program: number[]) {
    let enc = Encoding.Bech32;
    if (version > 0) {
      enc = Encoding.Bech32m;
    }
    const ret = bech32.encode(
      hrp,
      // @ts-expect-error can be null
      [version].concat(this.convertbits(program, 8, 5, true)),
      enc,
    );
    if (this.decode(hrp, ret) === null) {
      return null;
    }
    return ret;
  },
};

export const isValidSegwitAddress = (address: string) => {
  const hrp = /^(bc|tb|bcrt)1/.exec(address)?.[1];
  if (!hrp) return false;
  return segwitAddress.decode(hrp, address) !== null;
};
