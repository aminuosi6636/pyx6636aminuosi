import type { AppError } from './types';

function isAppError(value: unknown): value is AppError {
  if (typeof value !== 'object' || value === null) return false;
  return 'code' in value && typeof value.code === 'string'
    && 'message' in value && typeof value.message === 'string'
    && 'retryable' in value && typeof value.retryable === 'boolean';
}

export function toAppError(value: unknown): AppError {
  if (isAppError(value)) return value;
  // 不把底层异常、SQL 或路径直接展示给员工。
  return {
    code: 'UNEXPECTED',
    message: '操作未能完成。请重新打开软件；如果仍然出现，请联系维护人员查看日志。',
    retryable: false,
  };
}
