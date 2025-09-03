import './globals.css';

import * as Babel from '@babel/standalone';
import * as shadcnComponents from '@shinkai_network/shinkai-artifacts';
import { DotPattern } from '@shinkai_network/shinkai-ui';
import { Loader2 } from 'lucide-react';
import * as lucideReactIcons from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import * as React from 'react';
import ReactDOM from 'react-dom/client';
import * as recharts from 'recharts';

class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean; error: Error | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="whitespace-pre-wrap border bg-red-100 p-4 text-sm text-red-700">
          <h3 className="font-medium">Runtime Error:</h3>
          <pre>{this.state.error?.message}</pre>
        </div>
      );
    }

    return this.props.children;
  }
}

export const getReactComponentFromCode = (code: string) => {
  try {
    const transpiledCode = Babel.transform(code, {
      // presets: ['react'],
      presets: ['react', 'typescript'],
      filename: 'App.tsx',
      plugins: [importToVariablePlugin],
    }).code;

    const scope: any = {
      React: {
        ...React,
        useState: React.useState,
        useEffect: React.useEffect,
      },
      ...shadcnComponents,
      ...recharts,
      ...lucideReactIcons,
    };

    const fullCode = `
      const exports = {};
      ${transpiledCode}
      return exports.default;
    `;

    const evalCode = new Function('scope', fullCode);
    const ComponentToRender = evalCode(scope);

    return ComponentToRender;
  } catch (error) {
    console.error('Render Component: ' + error);
  }
};

const importToVariablePlugin = ({ types: t }: any) => ({
  visitor: {
    ImportDeclaration(path: any) {
      const declarations = path.node.specifiers
        .map((specifier: any) => {
          if (t.isImportDefaultSpecifier(specifier)) {
            return t.variableDeclarator(
              specifier.local,
              t.memberExpression(
                t.identifier('scope'),
                t.identifier(specifier.local.name),
              ),
            );
          } else if (t.isImportSpecifier(specifier)) {
            if (path.node.source.value === 'react') {
              return t.variableDeclarator(
                specifier.local,
                t.memberExpression(
                  t.memberExpression(
                    t.identifier('scope'),
                    t.identifier('React'),
                  ),
                  specifier.imported,
                ),
              );
            } else {
              return t.variableDeclarator(
                specifier.local,
                t.memberExpression(t.identifier('scope'), specifier.imported),
              );
            }
          }
          return null;
        })
        .filter(Boolean);

      path.replaceWith(t.variableDeclaration('const', declarations));
    },
    ExportDefaultDeclaration(path: any) {
      path.replaceWith(
        t.expressionStatement(
          t.assignmentExpression(
            '=',
            t.memberExpression(
              t.identifier('exports'),
              t.identifier('default'),
            ),
            path.node.declaration,
          ),
        ),
      );
    },
  },
});

const App = () => {
  const [code, setCode] = useState<string | null>(null);
  const [component, setComponent] = useState<React.ComponentType | null>(null);

  const contentRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (code) {
      try {
        const ComponentToRender = getReactComponentFromCode(code);
        if (ComponentToRender) {
          setComponent(() => ComponentToRender);
        } else {
          throw new Error(
            'No valid React component found in the provided code',
          );
        }
      } catch {
        throw new Error('Error evaluating component code');
      }
    }
  }, [code]);

  useEffect(() => {
    window.parent.postMessage({ type: 'INIT_COMPLETE' }, '*');

    const handleMessage = (event: any) => {
      if (event?.data?.type === 'UPDATE_COMPONENT') {
        setCode(event?.data?.code ?? '');
      }
    };

    window.addEventListener('message', handleMessage);

    return () => window.removeEventListener('message', handleMessage);
  }, []);

  return (
    <div className="min-h-full w-full" ref={contentRef}>
      {component ? (
        React.createElement(component)
      ) : (
        <div className="flex h-full items-center justify-center bg-white">
          <DotPattern className="[mask-image:linear-gradient(to_bottom_right,white,transparent,transparent)]" />
          <div className="flex items-center gap-1 text-sm">
            <Loader2 className="h-5 w-5 animate-spin text-gray-200" />
            <p className="text-gray-200">Loading...</p>
          </div>
        </div>
      )}
    </div>
  );
};

ReactDOM.createRoot(document.querySelector('#root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
