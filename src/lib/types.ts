export interface AppError {
  code: string;
  message: string;
  retryable: boolean;
}

export interface BootstrapReport {
  appVersion: string;
  databaseVersion: number;
  databaseReady: boolean;
  dataDirectory: string | null;
  logDirectory: string | null;
  issue: AppError | null;
}

export interface IngestCandidate { id: string; filename: string; size: number; status: 'ready' | 'waiting' | 'needs_review' | 'error'; }
export interface LibraryStatus {
  id: string | null; rootPath: string | null; scanning: boolean;
  total: number; ready: number; issue: AppError | null; candidates: IngestCandidate[];
  videos: Video[]; page: number;
}
export interface Video { id: string; filename: string; currentPath: string; useCount: number; status: 'ready' | 'missing' | 'needs_review' | 'error' | 'ingesting'; }
