// SPDX-License-Identifier: GPL-3.0-only
// The entry point: the one bundle serves the main window and every pop-out
// composer window. The URL's composer param picks the root: the app shell,
// or the composer window's own root over the composition it names.
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { ComposerWindow } from './features/composer/ComposerWindow';
import { readComposerParam } from './features/composer/composeWindow';
import './styles/globals.css';

const composition = readComposerParam(window.location.search);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {composition === null ? <App /> : <ComposerWindow composition={composition} />}
  </StrictMode>,
);
