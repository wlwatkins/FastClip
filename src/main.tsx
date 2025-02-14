import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { MantineProvider } from '@mantine/core';
import { mantineTheme } from "./theme";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MantineProvider withGlobalClasses withCssVariables theme={mantineTheme}>
      <App />
    </MantineProvider>
  </React.StrictMode>,
);
