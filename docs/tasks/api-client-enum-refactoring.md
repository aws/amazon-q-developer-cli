# ApiClient Enum Refactoring

**Status:** In Progress  
**Started:** 2025-12-31

## Problem Statement

Add an enum-based architecture to `ApiClient` that supports both real API calls and IPC-based mocking for E2E tests, without changing the public API or existing call sites.

## Requirements

- `ApiClient` keeps its current public interface
- Internal enum dispatches to `Real` (current implementation) or `IpcMock`
- Existing `cfg!(test)` stubs and `mock_client` field remain unchanged
- IPC mock supports `send_message` for E2E tests (other methods left as TODOs)

## Proposed Solution

1. Extract current `ApiClient` internals into a `RealApiClient` struct
2. Create `IpcMockApiClient` struct with TODO stubs (except `send_message`)
3. Add inner `enum ApiClientInner { Real(RealApiClient), IpcMock(IpcMockApiClient) }`
4. `ApiClient` delegates all methods to the inner enum
5. Move `IpcModel` actor pattern into `IpcMockApiClient` for `send_message`
6. Update `AcpSession` to use `IpcMockApiClient` instead of separate `IpcModel`

## Task Breakdown

### Task 1: Extract RealApiClient struct
- [ ] Create `RealApiClient` struct with all current fields
- [ ] Move `new()` to `RealApiClient::new()`
- [ ] Move all method implementations to `RealApiClient`
- [ ] Keep helper functions as module-level functions
- [ ] Verify existing tests pass

### Task 2: Create IpcMockApiClient stub
- [ ] Create `IpcMockApiClient` struct
- [ ] Add `todo!()` implementations for each API method
- [ ] Add `new()` constructor
- [ ] Verify code compiles

### Task 3: Create ApiClientInner enum and wire up ApiClient
- [ ] Create `enum ApiClientInner { Real(RealApiClient), IpcMock(IpcMockApiClient) }`
- [ ] Change `ApiClient` to hold `inner: ApiClientInner`
- [ ] `ApiClient::new()` creates `ApiClientInner::Real(...)`
- [ ] Each `ApiClient` method matches on `self.inner` and delegates
- [ ] Add `ApiClient::new_ipc_mock()` constructor
- [ ] Verify all existing tests pass

### Task 4: Implement IpcMockApiClient.send_message with actor pattern
- [ ] Add `IpcMockApiClientHandle` with `push_send_message_response()` method
- [ ] Add actor channel fields to `IpcMockApiClient` and spawn actor in `new()`
- [ ] Implement `send_message()` using buffered response pattern
- [ ] Update `AcpSession` to use `ApiClient::new_ipc_mock()` when `KIRO_TEST_MODE` is set
- [ ] Store `IpcMockApiClientHandle` instead of `IpcModelHandle`
- [ ] Create `RtsModel` with the `IpcMock` `ApiClient` variant
- [ ] Remove `IpcModel` and `ipc_model.rs`
- [ ] Update IPC server to call `handle.push_send_message_response()`
- [ ] Verify E2E tests pass

## Progress

### Task 1: Extract RealApiClient struct - ✅ Complete
- Created `RealApiClient` struct with all fields from original `ApiClient`
- Moved `new()` and all method implementations to `RealApiClient`
- Kept helper functions (`classify_error_kind`, `timeout_config`, etc.) as module-level functions

### Task 2: Create IpcMockApiClient stub - ✅ Complete
- Created `IpcMockApiClient` struct with actor channel
- Created `IpcMockApiClientTestHandle` for test-side mock injection
- Added `todo!()` implementations for all API methods except `send_message`
- Added actor pattern with `ipc_mock_actor` function

### Task 3: Create ApiClientInner enum and wire up ApiClient - ✅ Complete
- Created `enum ApiClientInner { Real(RealApiClient), IpcMock(IpcMockApiClient) }`
- Changed `ApiClient` to hold `inner: ApiClientInner`
- `ApiClient::new()` creates `ApiClientInner::Real(...)`
- Each `ApiClient` method delegates to inner enum
- Added `ApiClient::new_ipc_mock()` constructor

### Task 4: Implement IpcMockApiClient.send_message with actor pattern - ✅ Complete
- Implemented `send_message()` using buffered response pattern
- Added `IpcMock` variant to `SendMessageOutput` enum
- Updated `AcpSession` to use `ApiClient::new_ipc_mock()` when `KIRO_TEST_MODE` is set
- Removed `IpcModel` and `ipc_model.rs`
- Updated IPC server to use `ChatResponseStream` and `PushSendMessageResponse`
- Added `Serialize`/`Deserialize`/`typeshare` to `ChatResponseStream` for test file support
- Generated TypeScript types for E2E tests
- Updated E2E test case to use `ChatResponseStream` and `pushSendMessageResponse()`
- Updated IPC types in `ipc-types.ts`
- E2E tests pass
