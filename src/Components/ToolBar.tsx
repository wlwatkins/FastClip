import { getCurrentWindow } from '@tauri-apps/api/window';


function ToolBar() {

    const appWindow = getCurrentWindow();

    document.getElementById('titlebar-minimize')?.addEventListener('click', () => appWindow.minimize());
    document.getElementById('titlebar-maximize')?.addEventListener('click', () => appWindow.toggleMaximize());
    document.getElementById('titlebar-close')?.addEventListener('click', () => appWindow.close());

    return (
        <div data-tauri-drag-region className="titlebar bg-gray-700!">
            <div className="titlebar-button" id="titlebar-minimize">
                <img
                    src="https://api.iconify.design/mdi:window-minimize.svg"
                    alt="minimize"
                />
            </div>
            <div className="titlebar-button" id="titlebar-maximize">
                <img
                    src="https://api.iconify.design/mdi:window-maximize.svg"
                    alt="maximize"
                />
            </div>
            <div className="titlebar-button" id="titlebar-close">
                <img src="https://api.iconify.design/mdi:close.svg" alt="close" />
            </div>
        </div>
    );
}

export default ToolBar;



