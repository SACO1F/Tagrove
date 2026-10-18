import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

export async function pdfEngine() {
  const pdf = await import("pdfjs-dist");
  pdf.GlobalWorkerOptions.workerSrc = workerUrl;
  return pdf;
}

export function pdfOptions(data: Uint8Array) {
  return {
    data,
    useSystemFonts: true,
    isEvalSupported: false,
    cMapUrl: new URL("pdf-assets/cmaps/", document.baseURI).href,
    cMapPacked: true,
    standardFontDataUrl: new URL("pdf-assets/standard_fonts/", document.baseURI)
      .href,
    wasmUrl: new URL("pdf-assets/wasm/", document.baseURI).href,
  };
}
