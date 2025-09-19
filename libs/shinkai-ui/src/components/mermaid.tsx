import { save } from '@tauri-apps/plugin-dialog';
import * as fs from '@tauri-apps/plugin-fs';
import { RotateCcw, Download } from 'lucide-react';
import mermaid from 'mermaid';
import {
  type FC,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { toast } from 'sonner';
import svgPanZoom from 'svg-pan-zoom';
import { cn } from '../utils';
import { Button } from './button';
import { type SyntaxHighlighterProps } from './markdown-preview';
import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';

export type MermaidDiagramProps = SyntaxHighlighterProps & {
  className?: string;
  isRunning?: boolean;
};

mermaid.initialize({
  startOnLoad: false,
  mindmap: {
    useWidth: 800,
  },
  theme: 'dark',
  fontSize: 18,
  darkMode: true,
  fontFamily: 'Inter',
});

const generateId = () => `mermaid-${Math.random().toString(36).slice(2)}`;

const getErrorMessage = (error: unknown) => {
  if (!error) return 'Unknown Mermaid error';
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  if (typeof error === 'object' && 'str' in error) {
    const maybeStr = (error as { str?: unknown }).str;
    if (typeof maybeStr === 'string') return maybeStr;
  }
  return 'Unknown Mermaid error';
};

const downloadBlob = async (blob: Blob, filename: string) => {
  const filePath = await save({
    title: 'Save Mermaid Diagram',
    filters: [
      {
        name: 'SVG Files',
        extensions: ['svg'],
      },
    ],
    defaultPath: filename,
  });

  if (!filePath) return;

  const arrayBuffer = await blob.arrayBuffer();

  try {
    await fs.writeFile(filePath, new Uint8Array(arrayBuffer));
    toast.success('Mermaid diagram saved successfully');
  } catch (error) {
    console.warn('Failed to save Mermaid diagram:', error);
    toast.error(getErrorMessage(error));
  }
};

export const MermaidDiagram: FC<MermaidDiagramProps> = ({
  code,
  className,
  isRunning,
}) => {
  const ref = useRef<HTMLDivElement>(null);
  const lastCode = useRef<string | null>(null);
  const instanceRef = useRef<ReturnType<typeof svgPanZoom> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hasDiagram, setHasDiagram] = useState(false);

  const isComplete = !isRunning;

  const resetZoom = useCallback(() => {
    const instance = instanceRef.current;
    instance?.fit();
    instance?.center();
  }, []);

  const downloadSVG = useCallback(() => {
    if (error) return;

    if (ref.current) {
      const svg = ref.current.innerHTML;
      const blob = new Blob([svg], { type: 'image/svg+xml' });
      void downloadBlob(blob, 'mermaid-diagram.svg');
    }
  }, [error]);

  const enableZoom = useCallback(() => {
    const instance = instanceRef.current;
    instance?.enablePan();
    instance?.enableZoom();
  }, []);

  const disableZoom = useCallback(() => {
    const instance = instanceRef.current;
    instance?.disablePan();
    instance?.disableZoom();
  }, []);

  const handleFocus = useCallback(() => {
    enableZoom();
  }, [enableZoom]);

  const handleBlur = useCallback(() => {
    disableZoom();
  }, [disableZoom]);

  const handleClick = useCallback(() => {
    if (ref.current) {
      ref.current.focus();
    }
  }, []);

  useEffect(() => {
    if (!ref.current) return;

    if (!isComplete || !code) {
      if (ref.current) ref.current.innerHTML = '';
      instanceRef.current?.destroy();
      instanceRef.current = null;
      lastCode.current = null;
      setHasDiagram(false);
      setError(null);
      return;
    }

    if (lastCode.current === code) return;

    let cancelled = false;

    void (async () => {
      try {
        setError(null);
        const id = generateId();
        const { svg } = await mermaid.render(id, code);

        if (!ref.current || cancelled) return;

        instanceRef.current?.destroy();
        instanceRef.current = null;

        ref.current.innerHTML = svg;

        const svgElement = ref.current.querySelector('svg');
        if (svgElement) {
          svgElement.setAttribute('height', '100%');
          svgElement.setAttribute('width', '100%');
          svgElement.style.height = '100%';
          svgElement.style.width = '100%';
          svgElement.style.position = 'absolute';
          svgElement.style.top = '0';
          svgElement.style.left = '0';

          const panZoomInstance = svgPanZoom(svgElement);
          panZoomInstance.fit();
          panZoomInstance.center();
          panZoomInstance.zoomAtPoint(1, { x: 0, y: 0 });
          panZoomInstance.disablePan();
          panZoomInstance.disableZoom();
          instanceRef.current = panZoomInstance;
          setHasDiagram(true);
        } else {
          setHasDiagram(false);
        }

        lastCode.current = code;
      } catch (renderError) {
        if (cancelled) return;

        console.warn('Failed to render Mermaid diagram:', renderError);

        instanceRef.current?.destroy();
        instanceRef.current = null;
        lastCode.current = null;
        setHasDiagram(false);
        setError(getErrorMessage(renderError));
        if (ref.current) {
          ref.current.innerHTML = '';
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [code, isComplete]);
  
  useEffect(() => {
    return () => {
      instanceRef.current?.destroy();
      instanceRef.current = null;
    };
  }, []);

  return (
    <div className="relative">
      {hasDiagram && !error && (
        <div className="absolute top-2 right-2 z-10 flex gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="tertiary"
                size="icon"
                onClick={resetZoom}
                className="text-text-secondary"
              >
                <RotateCcw size={14} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Reset Zoom</p>
            </TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="tertiary"
                size="icon"
                onClick={downloadSVG}
                className="text-text-secondary"
              >
                <Download size={14} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Download SVG</p>
            </TooltipContent>
          </Tooltip>
        </div>
      )}
      {hasDiagram && !error && (
        <div
          onClick={handleClick}
          className="text-text-secondary bg-bg-quaternary absolute right-2 bottom-2 z-10 rounded px-2 py-1 text-xs"
        >
          Click to focus, then scroll to zoom & drag to pan
        </div>
      )}
      <div
        ref={ref}
        tabIndex={0}
        onFocus={handleFocus}
        onBlur={handleBlur}
        onClick={handleClick}
        className={cn(
          'text-text-secondary bg-bg-quaternary focus:ring-opacity-50 relative w-full cursor-grab overflow-hidden rounded-b-lg p-4 py-10 text-center text-sm focus:cursor-grabbing focus:ring-2 focus:ring-blue-500 focus:outline-none',
          className,
        )}
        style={{ width: '100%', minHeight: '400px' }}
      >
        {error ? (
          <div className="text-left">
            <p className="text-destructive font-semibold">
              Mermaid diagram failed to render
            </p>
            <p className="text-xs text-text-secondary">
              {error}
            </p>
            <div className="mt-4 text-left">
              <pre className="bg-bg-darker text-text-default overflow-x-auto rounded-md p-3 text-left text-xs">
                <code>{code}</code>
              </pre>
            </div>
          </div>
        ) : (
          'Drawing diagram...'
        )}
      </div>
    </div>
  );
};
