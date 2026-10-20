// SPDX-License-Identifier: GPL-3.0-only
// The entry point: the one bundle serves the main window and every pop-out
// composer window. The URL's composer param picks the root: the app shell,
// or the composer window's own root over the composition it names, with
// the mailto prefill and the transfer's resume account riding the URL
// beside it.
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { ComposerWindow } from './features/composer/ComposerWindow';
import {
  readComposerParam,
  readPrefillParam,
  readResumeParam,
} from './features/composer/composeWindow';
import './styles/globals.css';

const composition = readComposerParam(window.location.search);
const resumeAccount = readResumeParam(window.location.search);
const prefill = readPrefillParam(window.location.search);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {composition === null ? (
      <App />
    ) : (
      <ComposerWindow composition={composition} resumeAccount={resumeAccount} prefill={prefill} />
    )}
  </StrictMode>,
);
