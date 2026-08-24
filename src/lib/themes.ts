/** 界面主题：每套含浅色/深色两版，值覆盖 UI 变量与编辑器(nord)配色变量 */
export interface ThemeVars {
  '--bg': string;
  '--bg-panel': string;
  '--bg-sidebar': string;
  '--bg-hover': string;
  '--bg-active': string;
  '--border': string;
  '--text': string;
  '--text-dim': string;
  '--accent': string;
  '--danger': string;
  '--color-nord8': string;
  '--color-nord10': string;
  '--color-gray-50': string;
  '--color-gray-100': string;
  '--color-gray-200': string;
  '--color-gray-600': string;
  '--color-gray-700': string;
  '--color-gray-800': string;
  '--color-gray-900': string;
}

export interface Theme {
  id: string;
  name: string;
  light: ThemeVars;
  dark: ThemeVars;
}

const warmPaperLight: ThemeVars = {
  '--bg': '#faf9f7',
  '--bg-panel': '#f4f2ee',
  '--bg-sidebar': '#efece6',
  '--bg-hover': 'rgba(0, 0, 0, 0.055)',
  '--bg-active': 'rgba(0, 0, 0, 0.09)',
  '--border': '#e3dfd7',
  '--text': '#2f2b26',
  '--text-dim': '#8a8378',
  '--accent': '#b4753f',
  '--danger': '#c0392b',
  '--color-nord8': '#88c0d0',
  '--color-nord10': '#5e81ac',
  '--color-gray-50': '#fafaf9',
  '--color-gray-100': '#f5f5f4',
  '--color-gray-200': '#e7e5e4',
  '--color-gray-600': '#78716c',
  '--color-gray-700': '#57534e',
  '--color-gray-800': '#292524',
  '--color-gray-900': '#1c1917',
};

const warmPaperDark: ThemeVars = {
  '--bg': '#1e1c19',
  '--bg-panel': '#242220',
  '--bg-sidebar': '#2a2825',
  '--bg-hover': 'rgba(255, 255, 255, 0.06)',
  '--bg-active': 'rgba(255, 255, 255, 0.1)',
  '--border': '#3a3733',
  '--text': '#e8e2d8',
  '--text-dim': '#928a7c',
  '--accent': '#d99a63',
  '--danger': '#e06c5d',
  '--color-nord8': '#88c0d0',
  '--color-nord10': '#88c0d0',
  '--color-gray-50': '#292625',
  '--color-gray-100': '#242220',
  '--color-gray-200': '#3a3733',
  '--color-gray-600': '#a8a29e',
  '--color-gray-700': '#c8c2ba',
  '--color-gray-800': '#e8e2d8',
  '--color-gray-900': '#f5f3ef',
};

const indigoLight: ThemeVars = {
  '--bg': '#f7f8fb',
  '--bg-panel': '#f0f2f7',
  '--bg-sidebar': '#e9ecf4',
  '--bg-hover': 'rgba(60, 80, 140, 0.07)',
  '--bg-active': 'rgba(60, 80, 140, 0.12)',
  '--border': '#dde2ec',
  '--text': '#262a35',
  '--text-dim': '#7d8496',
  '--accent': '#4f6d9e',
  '--danger': '#c0392b',
  '--color-nord8': '#81a1c1',
  '--color-nord10': '#4f6d9e',
  '--color-gray-50': '#f8fafc',
  '--color-gray-100': '#f1f5f9',
  '--color-gray-200': '#e2e8f0',
  '--color-gray-600': '#64748b',
  '--color-gray-700': '#334155',
  '--color-gray-800': '#1e293b',
  '--color-gray-900': '#0f172a',
};

const indigoDark: ThemeVars = {
  '--bg': '#171a21',
  '--bg-panel': '#1d212b',
  '--bg-sidebar': '#232834',
  '--bg-hover': 'rgba(150, 170, 220, 0.07)',
  '--bg-active': 'rgba(150, 170, 220, 0.13)',
  '--border': '#333a49',
  '--text': '#dbe2ef',
  '--text-dim': '#8b93a8',
  '--accent': '#8aa8d8',
  '--danger': '#e06c5d',
  '--color-nord8': '#81a1c1',
  '--color-nord10': '#81a1c1',
  '--color-gray-50': '#232834',
  '--color-gray-100': '#1d212b',
  '--color-gray-200': '#333a49',
  '--color-gray-600': '#9aa4bd',
  '--color-gray-700': '#c3cbe0',
  '--color-gray-800': '#dbe2ef',
  '--color-gray-900': '#eef1f8',
};

const bambooLight: ThemeVars = {
  '--bg': '#f8faf6',
  '--bg-panel': '#f1f5ee',
  '--bg-sidebar': '#ebf1e8',
  '--bg-hover': 'rgba(60, 110, 70, 0.07)',
  '--bg-active': 'rgba(60, 110, 70, 0.12)',
  '--border': '#dfe8db',
  '--text': '#27302a',
  '--text-dim': '#7d8a80',
  '--accent': '#5b8c5a',
  '--danger': '#c0392b',
  '--color-nord8': '#8fbcbb',
  '--color-nord10': '#5b8c5a',
  '--color-gray-50': '#f8faf6',
  '--color-gray-100': '#f0f5ec',
  '--color-gray-200': '#e0e8da',
  '--color-gray-600': '#6b7a6f',
  '--color-gray-700': '#4a584e',
  '--color-gray-800': '#27332b',
  '--color-gray-900': '#18211c',
};

const bambooDark: ThemeVars = {
  '--bg': '#161d18',
  '--bg-panel': '#1c2520',
  '--bg-sidebar': '#222d26',
  '--bg-hover': 'rgba(140, 200, 150, 0.07)',
  '--bg-active': 'rgba(140, 200, 150, 0.13)',
  '--border': '#33412f',
  '--text': '#dbe8d8',
  '--text-dim': '#8ba088',
  '--accent': '#8fbe8e',
  '--danger': '#e06c5d',
  '--color-nord8': '#8fbcbb',
  '--color-nord10': '#8fbcbb',
  '--color-gray-50': '#222d26',
  '--color-gray-100': '#1c2520',
  '--color-gray-200': '#33412f',
  '--color-gray-600': '#9bb29a',
  '--color-gray-700': '#c2d4be',
  '--color-gray-800': '#dbe8d8',
  '--color-gray-900': '#eef5ec',
};

const inkLight: ThemeVars = {
  '--bg': '#f6f6f7',
  '--bg-panel': '#ededef',
  '--bg-sidebar': '#e7e7ea',
  '--bg-hover': 'rgba(40, 40, 50, 0.06)',
  '--bg-active': 'rgba(40, 40, 50, 0.1)',
  '--border': '#dcDce0',
  '--text': '#232326',
  '--text-dim': '#7c7c85',
  '--accent': '#5a5a68',
  '--danger': '#c0392b',
  '--color-nord8': '#7f8ba3',
  '--color-nord10': '#5a5a68',
  '--color-gray-50': '#f8f8f9',
  '--color-gray-100': '#f0f0f2',
  '--color-gray-200': '#e4e4e8',
  '--color-gray-600': '#6f6f78',
  '--color-gray-700': '#4a4a52',
  '--color-gray-800': '#27272b',
  '--color-gray-900': '#18181c',
};

const inkDark: ThemeVars = {
  '--bg': '#141417',
  '--bg-panel': '#1a1a1e',
  '--bg-sidebar': '#202024',
  '--bg-hover': 'rgba(255, 255, 255, 0.06)',
  '--bg-active': 'rgba(255, 255, 255, 0.1)',
  '--border': '#303036',
  '--text': '#dcdce2',
  '--text-dim': '#8a8a94',
  '--accent': '#a5a5b2',
  '--danger': '#e06c5d',
  '--color-nord8': '#9aa3b8',
  '--color-nord10': '#9aa3b8',
  '--color-gray-50': '#202024',
  '--color-gray-100': '#1a1a1e',
  '--color-gray-200': '#303036',
  '--color-gray-600': '#9c9ca6',
  '--color-gray-700': '#c4c4cd',
  '--color-gray-800': '#dcdce2',
  '--color-gray-900': '#eff0f4',
};

export const THEMES: Theme[] = [
  { id: 'warm-paper', name: '暖纸', light: warmPaperLight, dark: warmPaperDark },
  { id: 'indigo', name: '黛蓝', light: indigoLight, dark: indigoDark },
  { id: 'bamboo', name: '竹青', light: bambooLight, dark: bambooDark },
  { id: 'ink', name: '墨韵', light: inkLight, dark: inkDark },
];

export function themeById(id: string): Theme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}
