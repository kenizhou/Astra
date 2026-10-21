// SPDX-License-Identifier: GPL-3.0-only
// The plugin IPC layer: the registry's list/enable/disable/approve verbs and
// the one call every plugin surface (composer actions, settings fields, the
// frame's bridge) funnels through. The wire shapes mirror the desktop host's
// serde exactly: a row carries its descriptors inline when the manifest
// declared any, and a call's answer is the worker's own payload.

import { invoke } from '@tauri-apps/api/core';

/** One composer action a plugin declared: a button in the composer chrome. */
export interface ComposerActionDescriptor {
  /** The action's id; its click calls the worker's `plugin.<id>.<id>` method. */
  id: string;
  /** Its label. */
  label: string;
  /** Its icon name, when it carries one. */
  icon?: string;
}

/** The kind of one settings field a plugin declared. */
export type SettingsFieldKind = 'toggle' | 'text' | 'select' | 'password' | 'action';

/** One field of a plugin's settings section. */
export interface SettingsFieldDescriptor {
  /** The field's key, the name the worker's answers name it by. */
  key: string;
  /** Its kind. */
  kind: SettingsFieldKind;
  /** Its label. */
  label: string;
  /** The choices a select field offers; empty for every other kind. */
  options: string[];
}

/** One settings section a plugin declared: a category in the settings screen. */
export interface SettingsSectionDescriptor {
  /** The section's id. */
  id: string;
  /** Its title. */
  title: string;
  /** Its fields. */
  fields: SettingsFieldDescriptor[];
}

/** One rich page a plugin declared, rendered inside the sandboxed frame. */
export interface PageDescriptor {
  /** The page's id. */
  id: string;
  /** Its title. */
  title: string;
  /** Its entry HTML inside the plugin's bundle. */
  entry: string;
}

/** The UI descriptors a plugin's manifest declared. */
export interface UiDescriptors {
  /** The composer chrome buttons. */
  composer_actions: ComposerActionDescriptor[];
  /** The settings categories. */
  settings_sections: SettingsSectionDescriptor[];
  /** The rich pages for the sandboxed frame. */
  pages: PageDescriptor[];
}

/** One registry row as the host reports it. */
export interface PluginRow {
  /** The plugin's id. */
  id: string;
  /** Its display name. */
  name: string;
  /** Its version. */
  version: string;
  /** Its lifecycle state (`discovered`, `activated`, `failed`, ...). */
  lifecycle: string;
  /** The permissions the host has granted, as wire names. */
  grants: string[];
  /** The worker program the entry resolves to. */
  program: string;
  /** The UI descriptors the manifest declared, when it declared any. */
  ui?: UiDescriptors;
}

/** Lists the discovered plugins. */
export function pluginList(): Promise<PluginRow[]> {
  return invoke('plugin_list');
}

/** Enables one discovered plugin: loads it and starts its worker. */
export function pluginEnable(id: string): Promise<void> {
  return invoke('plugin_enable', { id });
}

/** Disables one active plugin: its worker is asked to exit and killed. */
export function pluginDisable(id: string): Promise<void> {
  return invoke('plugin_disable', { id });
}

/** Grants one manifest-declared permission to one plugin. */
export function pluginApprove(id: string, permission: string): Promise<void> {
  return invoke('plugin_approve', { id, permission });
}

/**
 * Calls one activated plugin's declared method: the composer actions' and
 * settings fields' whole reach, and the frame bridge's. The answer is the
 * worker's own result payload.
 */
export function pluginCall(id: string, method: string, args: unknown): Promise<unknown> {
  return invoke('plugin_call', { id, method, args });
}
