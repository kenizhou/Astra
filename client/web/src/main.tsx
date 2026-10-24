// SPDX-License-Identifier: GPL-3.0-only
// The entry point: the one bundle serves the main window and every pop-out
// window. The URL's params pick the root: the app shell, the composer
// window's own root over the composition it names, the reading window's
// root over the message it names, the print surface over the same message,
// with the mailto prefill, the transfer's resume account, and the folder
// the message was opened from riding the URL beside them.
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { ComposerWindow } from './features/composer/ComposerWindow';
import {
  readComposerParam,
  readPrefillParam,
  readResumeParam,
} from './features/composer/composeWindow';
import { MessageWindow } from './features/reader/MessageWindow';
import { PrintWindow } from './features/reader/PrintWindow';
import { readMessageWindowParams } from './features/reader/readerWindows';
import './styles/globals.css';

const composition = readComposerParam(window.location.search);
const resumeAccount = readResumeParam(window.location.search);
const prefill = readPrefillParam(window.location.search);
const messageWindow = readMessageWindowParams(window.location.search);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {messageWindow !== null ? (
      messageWindow.print ? (
        <PrintWindow message={messageWindow.message} />
      ) : (
        <MessageWindow message={messageWindow.message} folderKey={messageWindow.folderKey} />
      )
    ) : composition === null ? (
      <App />
    ) : (
      <ComposerWindow composition={composition} resumeAccount={resumeAccount} prefill={prefill} />
    )}
  </StrictMode>,
);
