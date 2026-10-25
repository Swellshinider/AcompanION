/// <reference types="@angular/localize" />

import { LOCALE_ID } from '@angular/core';
import { bootstrapApplication } from '@angular/platform-browser';
import { appConfig } from './app/app.config';
import { App } from './app/app';
import { initLocale } from './app/locale';

initLocale()
  .then((locale) => {
    document.documentElement.lang = locale;
    return bootstrapApplication(App, { providers: [...appConfig.providers, { provide: LOCALE_ID, useValue: locale }] });
  })
  .catch((err) => console.error(err));
