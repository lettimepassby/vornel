// “关于与开源许可”页面的数据。字体许可全文取自各字体发布包中的原始 LICENSE 文件。
import geist from '../assets/licenses/geist.txt?raw';
import geistMono from '../assets/licenses/geist-mono.txt?raw';
import inter from '../assets/licenses/inter.txt?raw';
import jetbrainsMono from '../assets/licenses/jetbrains-mono.txt?raw';
import manrope from '../assets/licenses/manrope.txt?raw';
import instrumentSans from '../assets/licenses/instrument-sans.txt?raw';
import spaceGrotesk from '../assets/licenses/space-grotesk.txt?raw';
import martianMono from '../assets/licenses/martian-mono.txt?raw';
import cascadiaCode from '../assets/licenses/cascadia-code.txt?raw';
import plexSans from '../assets/licenses/ibm-plex-sans.txt?raw';
import plexMono from '../assets/licenses/ibm-plex-mono.txt?raw';
import commitMono from '../assets/licenses/commit-mono.txt?raw';
import mapleMono from '../assets/licenses/maple-mono.txt?raw';
import monaspace from '../assets/licenses/monaspace-neon.txt?raw';

export interface FontLicense {
  name: string;
  text: string;
}

export const FONT_LICENSES: FontLicense[] = [
  { name: 'Geist', text: geist },
  { name: 'Geist Mono', text: geistMono },
  { name: 'Inter', text: inter },
  { name: 'JetBrains Mono', text: jetbrainsMono },
  { name: 'Manrope', text: manrope },
  { name: 'Instrument Sans', text: instrumentSans },
  { name: 'Space Grotesk', text: spaceGrotesk },
  { name: 'Martian Mono', text: martianMono },
  { name: 'Cascadia Code', text: cascadiaCode },
  { name: 'IBM Plex Sans', text: plexSans },
  { name: 'IBM Plex Mono', text: plexMono },
  { name: 'Commit Mono', text: commitMono },
  { name: 'Maple Mono', text: mapleMono },
  { name: 'Monaspace Neon', text: monaspace },
];

/** 主要依赖及其许可（完整清单见仓库中的 Cargo.lock 与 package-lock.json）。 */
export const LIBRARIES: { name: string; license: string; url: string }[] = [
  { name: 'Tauri', license: 'MIT / Apache-2.0', url: 'https://github.com/tauri-apps/tauri' },
  { name: 'Svelte', license: 'MIT', url: 'https://github.com/sveltejs/svelte' },
  { name: 'xterm.js', license: 'MIT', url: 'https://github.com/xtermjs/xterm.js' },
  { name: 'russh / russh-sftp', license: 'Apache-2.0', url: 'https://github.com/Eugeny/russh' },
  { name: 'IronRDP', license: 'MIT / Apache-2.0', url: 'https://github.com/Devolutions/IronRDP' },
  { name: 'vnc-rs', license: 'MIT / Apache-2.0', url: 'https://github.com/HsuJv/vnc-rs' },
  { name: 'serialport-rs', license: 'MPL-2.0', url: 'https://github.com/serialport/serialport-rs' },
  { name: 'Apache OpenDAL', license: 'Apache-2.0', url: 'https://github.com/apache/opendal' },
  { name: 'rusqlite / SQLite', license: 'MIT / Public Domain', url: 'https://github.com/rusqlite/rusqlite' },
  { name: 'RustCrypto（argon2、chacha20poly1305、aes、ocb3）', license: 'MIT / Apache-2.0', url: 'https://github.com/RustCrypto' },
  { name: 'keyring-rs', license: 'MIT / Apache-2.0', url: 'https://github.com/open-source-cooperative/keyring-rs' },
  { name: 'tokio', license: 'MIT', url: 'https://github.com/tokio-rs/tokio' },
  { name: 'Simple Icons（主机列表中的系统 logo，商标归各自所有者）', license: 'CC0-1.0', url: 'https://github.com/simple-icons/simple-icons' },
  { name: 'rustls / ring', license: 'Apache-2.0 / ISC / MIT', url: 'https://github.com/rustls/rustls' },
  { name: 'Fontsource', license: 'MIT（打包脚本）', url: 'https://github.com/fontsource/fontsource' },
];
