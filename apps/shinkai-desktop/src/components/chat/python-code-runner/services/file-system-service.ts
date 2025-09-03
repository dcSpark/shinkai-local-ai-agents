import { type PyodideInterface } from 'pyodide';

export type FileSystemEntry = {
  name: string;
  type: 'directory' | 'file';
  content?: string;
  contents?: FileSystemEntry[];
  mtimeMs?: number;
};

export interface IFileSystemService {
  initialize(): Promise<void>;
  readContents(path: string): FileSystemEntry[] | null;
  readContentsWithMtime(path: string): FileSystemEntry[] | null;
  writeFile(path: string, content: string): void;
  readFile(path: string): string | undefined;
  ensureDirectory(dirPath: string): void;
  removeStaleItems(dirPath: string, validPaths: Set<string>): void;
  syncToIndexedDB(): Promise<void>;
  syncFromIndexedDB(): Promise<void>;
}

export class PyodideFileSystemService implements IFileSystemService {
  private pyodide: PyodideInterface;
  private rootPath: string;

  constructor(pyodide: PyodideInterface, rootPath = '/home/pyodide') {
    this.pyodide = pyodide;
    this.rootPath = rootPath;
  }

  async initialize(): Promise<void> {
    try {
      this.pyodide.FS.mount(
        this.pyodide.FS.filesystems.IDBFS,
        {},
        this.rootPath
      );
      await this.syncFromIndexedDB();
    } catch (error) {
      console.error('Failed to initialize file system:', error);
      throw error;
    }
  }

  readContents(path: string): FileSystemEntry[] | null {
    try {
      const entries = this.pyodide.FS.readdir(path);
      const contents = entries.filter((entry: string) => 
        entry !== '.' && entry !== '..' && entry !== '.matplotlib'
      );
      
      return contents.map((entry: string) => {
        const fullPath = `${path}/${entry}`;
        const stat = this.pyodide.FS.stat(fullPath);
        const isDirectory = this.pyodide.FS.isDir(stat.mode);
        
        if (isDirectory) {
          return { 
            name: entry, 
            type: 'directory' as const, 
            contents: this.readContents(fullPath) 
          };
        } else {
          const content = this.pyodide.FS.readFile(fullPath, { encoding: 'utf8' });
          return { 
            name: entry, 
            type: 'file' as const, 
            content: content as string 
          };
        }
      });
    } catch (error) {
      console.error(`Error reading ${path}:`, error);
      return null;
    }
  }

  readContentsWithMtime(path: string): FileSystemEntry[] | null {
    try {
      const entries = this.pyodide.FS.readdir(path);
      const contents = entries.filter((entry: string) => 
        entry !== '.' && entry !== '..' && entry !== '.matplotlib'
      );

      return contents.map((entry: string) => {
        const fullPath = `${path}/${entry}`;
        const stat = this.pyodide.FS.stat(fullPath);
        const isDirectory = this.pyodide.FS.isDir(stat.mode);
        const mtimeMs = stat ? stat.mtime * 1000 : 0;

        if (isDirectory) {
          return { 
            name: entry, 
            type: 'directory' as const, 
            contents: this.readContentsWithMtime(fullPath) 
          };
        } else {
          const content = this.pyodide.FS.readFile(fullPath, { encoding: 'utf8' });
          return { 
            name: entry, 
            type: 'file' as const, 
            content: content as string,
            mtimeMs
          };
        }
      });
    } catch (error) {
      console.error(`Error reading ${path}:`, error);
      return null;
    }
  }

  writeFile(path: string, content: string): void {
    try {
      this.pyodide.FS.writeFile(path, content, { encoding: 'utf8' });
    } catch (error) {
      console.error(`Failed to write file ${path}:`, error);
      throw error;
    }
  }

  readFile(path: string): string | undefined {
    try {
      return this.pyodide.FS.readFile(path, { encoding: 'utf8' });
    } catch (error) {
      console.error(`Failed to read file ${path}:`, error);
      return undefined;
    }
  }

  ensureDirectory(dirPath: string): void {
    if (dirPath === this.rootPath) {
      return;
    }

    const parts = dirPath.split('/').filter(Boolean);
    let currentPath = '';
    
    for (const part of parts) {
      currentPath += '/' + part;
      try {
        const stat = this.pyodide.FS.stat(currentPath);
        if (!this.pyodide.FS.isDir(stat.mode)) {
          this.pyodide.FS.unlink(currentPath);
          this.pyodide.FS.mkdir(currentPath);
        }
      } catch {
        try {
          this.pyodide.FS.mkdir(currentPath);
        } catch (mkdirErr) {
          console.error(`Failed to create directory ${currentPath}:`, mkdirErr);
          throw mkdirErr;
        }
      }
    }
  }

  removeStaleItems(dirPath: string, validPaths: Set<string>): void {
    const entries = this.pyodide.FS.readdir(dirPath);
    for (const entry of entries) {
      if (entry === '.' || entry === '..') continue;
      const fullPath = `${dirPath}/${entry}`;
      const stat = this.pyodide.FS.stat(fullPath);
      if (this.pyodide.FS.isDir(stat.mode)) {
        this.removeStaleItems(fullPath, validPaths);
        if (!validPaths.has(fullPath)) {
          try {
            this.pyodide.FS.rmdir(fullPath);
          } catch (err) {
            console.error(`Failed removing directory ${fullPath}:`, err);
          }
        }
      } else {
        if (!validPaths.has(fullPath)) {
          try {
            this.pyodide.FS.unlink(fullPath);
          } catch (err) {
            console.error(`Failed removing file ${fullPath}:`, err);
          }
        }
      }
    }
  }

  async syncToIndexedDB(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      this.pyodide.FS.syncfs(false, (err: Error | null) => {
        if (err) {
          console.error('Failed to sync to IndexedDB:', err);
          reject(err);
        } else {
          console.log('Successfully synced to IndexedDB');
          resolve();
        }
      });
    });
  }

  async syncFromIndexedDB(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      this.pyodide.FS.syncfs(true, (err: Error | null) => {
        if (err) {
          console.error('Failed to sync from IndexedDB:', err);
          reject(err);
        } else {
          console.log('Successfully synced from IndexedDB');
          resolve();
        }
      });
    });
  }
} 