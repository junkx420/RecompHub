import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import de from "./locales/de.json";
import en from "./locales/en.json";
import type { Language, LocalizedText } from "../api/types";

export const resources = { en: { translation: en }, de: { translation: de } } as const;
export type SupportedLanguage = keyof typeof resources;

export function systemLanguage(): SupportedLanguage {
  const preferred = typeof navigator === "undefined" ? [] : navigator.languages;
  for (const tag of preferred) {
    const base = tag.toLowerCase().split("-")[0];
    if (base === "de" || base === "en") return base;
  }
  return "en";
}

export function resolveLanguage(setting: Language): SupportedLanguage {
  return setting === "system" ? systemLanguage() : setting;
}

void i18n.use(initReactI18next).init({
  resources,
  lng: systemLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnNull: false,
});

/** Pick the text for `lang` from a catalog text, falling back to English. */
export function localized(text: LocalizedText | undefined, lang: string): string {
  if (text === undefined) return "";
  if (typeof text === "string") return text;
  return text[lang] ?? text["en"] ?? "";
}

export default i18n;
