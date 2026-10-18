import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { useReaderZoom } from "./ReaderZoom";
import bridge from "./reader-zoom-bridge.js?raw";
import { useReadingPosition } from "./ReadingState";

/** The opaque frame runs only this nonce-authorized gesture bridge, never file scripts. */
export function ZoomFrame({
  title,
  document,
  stateKey = "frame",
}: {
  title: string;
  document: string;
  stateKey?: string;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const token = useMemo(() => crypto.randomUUID().replaceAll("-", ""), []);
  const { scale, step } = useReaderZoom();
  const position = useReadingPosition();
  const reading = useRef(position);
  reading.current = position;
  const restored = useRef(false);
  const current = useRef({ scale, step });
  current.current = { scale, step };
  const send = () =>
    frame.current?.contentWindow?.postMessage(
      { type: "shiqian-zoom", token, scale: current.current.scale },
      "*",
    );
  const sendAppearance = () => {
    const accent = getComputedStyle(window.document.documentElement)
      .getPropertyValue("--accent-light")
      .trim();
    frame.current?.contentWindow?.postMessage(
      { type: "shiqian-appearance", token, accent },
      "*",
    );
  };
  const sync = () => {
    send();
    sendAppearance();
    if (!restored.current) {
      frame.current?.contentWindow?.postMessage(
        {
          type: "shiqian-position",
          token,
          top: reading.current.get(stateKey + "Top"),
          left: reading.current.get(stateKey + "Left"),
        },
        "*",
      );
      restored.current = true;
    }
  };
  const content = useMemo(() => {
    return document
      .replace("script-src 'none'", `script-src 'nonce-${token}'`)
      .replace(
        /<\/body>/i,
        `<script nonce="${token}" data-token="${token}">${bridge}</script></body>`,
      );
  }, [document, token]);
  useLayoutEffect(() => {
    restored.current = false;
  }, [content]);
  useEffect(() => {
    const receive = (event: MessageEvent) => {
      if (
        event.source !== frame.current?.contentWindow ||
        event.origin !== "null" ||
        event.data?.token !== token
      )
        return;
      if (event.data.type === "shiqian-zoom-ready") sync();
      if (
        event.data.type === "shiqian-position" &&
        Number.isFinite(event.data.top) &&
        Number.isFinite(event.data.left)
      ) {
        reading.current.set(stateKey + "Top", event.data.top);
        reading.current.set(stateKey + "Left", event.data.left);
      }
      if (
        event.data.type === "shiqian-zoom-wheel" &&
        (event.data.direction === 1 || event.data.direction === -1)
      )
        current.current.step(event.data.direction);
    };
    window.addEventListener("message", receive);
    return () => window.removeEventListener("message", receive);
  }, [token]);
  useEffect(() => {
    send();
    sendAppearance();
  }, [scale, content]);
  useEffect(() => {
    const observer = new MutationObserver(sendAppearance);
    observer.observe(window.document.documentElement, {
      attributes: true,
      attributeFilter: ["data-accent", "data-theme"],
    });
    return () => observer.disconnect();
  }, [token]);
  return (
    <iframe
      ref={frame}
      data-tooltip={title}
      aria-label={title}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      srcDoc={content}
      onLoad={sync}
    />
  );
}
