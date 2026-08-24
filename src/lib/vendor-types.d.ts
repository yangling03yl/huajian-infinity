// 第三方包缺少类型声明，这里给出最小可用的类型
declare module 'html-to-pdfmake' {
  export interface HtmlToPdfmakeOptions {
    defaultStyles?: Record<string, Record<string, unknown>>;
    window?: unknown;
    tableAutoSize?: boolean;
  }
  const htmlToPdfmake: (
    html: string,
    options?: HtmlToPdfmakeOptions,
  ) => Array<Record<string, unknown>>;
  export default htmlToPdfmake;
}

declare module 'pdfmake/build/pdfmake' {
  const pdfMake: {
    vfs: Record<string, string>;
    fonts: Record<string, unknown>;
    createPdf: (docDefinition: Record<string, unknown>) => {
      getBlob: (cb: (blob: Blob) => void) => void;
    };
  };
  export default pdfMake;
}
