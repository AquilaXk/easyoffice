// EasyOffice: the password-protected HWP open flow, split out of main.ts. The password stays inside
// openPasswordProtectedDocument: it is never stored, logged, sent over the embed port or passed on.
import type { WasmBridge } from '@/core/wasm-bridge';
import type { DocumentInfo } from '@/core/types';
import { showHwpPasswordDialog } from '@/ui/hwp-password-dialog';

/**
 * 사용자가 암호 입력 대화상자에서 취소한 경우다. 일반 파싱 실패와 달리 오류 토스트나
 * 최근 문서·자동저장 변경을 만들지 않는다 (#3474).
 */
export class DocumentOpenCancelledError extends Error {
  constructor() {
    super('문서 열기가 취소되었습니다.');
    this.name = 'DocumentOpenCancelledError';
  }
}

const PASSWORD_REQUIRED_MESSAGE = '비밀번호가 필요한 암호 문서';
const PASSWORD_REJECTED_MESSAGE = '비밀번호가 일치하지 않거나 암호화 데이터가 손상되었습니다';

export function isDocumentOpenCancelled(error: unknown): error is DocumentOpenCancelledError {
  return error instanceof DocumentOpenCancelledError;
}

function isPasswordRequiredError(error: unknown): boolean {
  return String(error).includes(PASSWORD_REQUIRED_MESSAGE);
}

function isPasswordRejectedError(error: unknown): boolean {
  return String(error).includes(PASSWORD_REJECTED_MESSAGE);
}

function passwordOpenFailure(error: unknown): Error {
  const message = String(error);
  if (message.includes('지원하지 않는 암호화 방식')) {
    return new Error('지원하지 않는 암호화 방식의 문서입니다. 지원되는 HWP3/HWP5 암호 문서만 열 수 있습니다.');
  }
  if (message.includes('DRM')) {
    return new Error('DRM으로 보호된 문서는 지원하지 않습니다.');
  }
  // 입력값이 포함될 수 있는 원본 오류는 사용자 화면이나 콘솔에 전달하지 않는다. 현재
  // 암호화 포맷은 오입력과 암호문 훼손을 암호학적으로 판별할 수 없으므로 안전한 일반
  // 안내로 축약한다.
  return new Error('암호화된 문서를 열 수 없습니다. 문서가 손상되었는지 확인하세요.');
}

/**
 * 일반 열기를 먼저 시도하고, 지원되는 HWP3/HWP5 암호 문서가 감지된 경우에만 암호
 * 입력 UI로 전환한다. 암호 문자열은 이 함수의 단일 시도 범위를 벗어나 보관하지 않는다.
 */
async function openPasswordProtectedDocument(wasm: WasmBridge, data: Uint8Array, fileName: string): Promise<void> {
  let retryMessage: string | undefined;

  while (true) {
    let password = await showHwpPasswordDialog(fileName, retryMessage);
    if (password === null) throw new DocumentOpenCancelledError();

    try {
      wasm.loadDocumentWithPassword(data, password, fileName);
      return;
    } catch (error) {
      // CFB 암호문은 인증 태그가 없으므로 오입력과 암호화 데이터 손상을 완전히 구분할 수
      // 없다. 두 경우만 재입력 상태로 안내하고, 지원하지 않는 암호화/DRM 등은 원래의
      // 명시적 거부 오류를 유지한다.
      if (isPasswordRejectedError(error)) {
        retryMessage = '암호가 일치하지 않거나 문서가 손상되었습니다. 다시 입력하세요.';
        continue;
      }
      throw passwordOpenFailure(error);
    } finally {
      // JavaScript 문자열을 확실히 zeroize할 수는 없지만, 대화상자 DOM과 이 지역 참조는
      // 시도 직후 해제한다. 최근 문서·URL·저장소·문서 메타데이터에는 전달하지 않는다.
      password = '';
    }
  }
}

export async function loadDocumentForOpen(wasm: WasmBridge, data: Uint8Array, fileName: string): Promise<DocumentInfo> {
  try {
    return wasm.loadDocument(data, fileName);
  } catch (error) {
    if (!isPasswordRequiredError(error)) throw error;
    await openPasswordProtectedDocument(wasm, data, fileName);
    return wasm.getDocumentInfo();
  }
}
