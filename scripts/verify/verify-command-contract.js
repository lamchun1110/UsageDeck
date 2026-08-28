import fs from 'node:fs';

const root = new URL('../../', import.meta.url);
const backend = fs.readFileSync(new URL('src/lib/backend.ts', root), 'utf8');
const application = fs.readFileSync(new URL('src-tauri/src/lib.rs', root), 'utf8');
const invokedCommands = new Set(
  [...backend.matchAll(/invoke(?:<[^>]+>)?\(\s*'([^']+)'/g)].map((match) => match[1]),
);
const handlerBlock = application.match(/invoke_handler\(tauri::generate_handler!\[([\s\S]*?)\]\)/);
if (!handlerBlock) {
  throw new Error('Could not find the Tauri invoke handler registration.');
}
const registeredCommands = new Set(
  [...handlerBlock[1].matchAll(/(?:[a-zA-Z0-9_]+::)*([a-zA-Z0-9_]+)/g)].map((match) => match[1]),
);

const missingCommands = [...invokedCommands].filter((command) => !registeredCommands.has(command));
if (missingCommands.length > 0) {
  throw new Error(`Frontend invokes unregistered Tauri commands: ${missingCommands.join(', ')}`);
}

// Commands the frontend never invokes are dead registrations unless something
// else drives them; list each exception here with its driver.
const rustDrivenCommands = new Set([
  // None today: the tray menu and refresh loop call shared helpers directly
  // rather than the #[tauri::command] wrappers.
]);
const deadCommands = [...registeredCommands].filter(
  (command) => !invokedCommands.has(command) && !rustDrivenCommands.has(command),
);
if (deadCommands.length > 0) {
  throw new Error(
    `Registered Tauri commands the frontend never invokes: ${deadCommands.join(', ')}`,
  );
}

console.log(
  `${invokedCommands.size} frontend Tauri commands match registered Rust command handlers ` +
    `(no dead registrations).`,
);
