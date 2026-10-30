# Run the Storybook workbench

From `web/`, install dependencies and start Storybook:

```sh
npm install
npm run storybook
```

Storybook opens `http://localhost:6006` in the local browser. Use its Controls
panel to change a story's environmental inputs and its Accessibility panel for
an interactive accessibility inspection. The upstream
[Controls](https://storybook.js.org/docs/essentials/controls) and
[accessibility](https://storybook.js.org/docs/writing-tests/accessibility-testing)
documentation covers the general interface.

For a remote machine or agent environment, leave browser opening disabled:

```sh
npm run storybook -- --no-open
```

Forward the remote loopback port from the computer where the browser runs:

```sh
ssh -N -L 6006:127.0.0.1:6006 user@remote-host
```

Then open `http://127.0.0.1:6006` locally. If local port 6006 is occupied, use
another local port, for example `-L 16006:127.0.0.1:6006`, and open that port.

Press Ctrl+C in the terminal running Storybook to stop it.
