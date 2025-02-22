import { ActionIcon } from '@mantine/core';
import { IconMinus, IconSquare, IconX } from '@tabler/icons-react';
import { getCurrentWindow } from '@tauri-apps/api/window';


function ToolBar() {

    const appWindow = getCurrentWindow();

    return (
        <div data-tauri-drag-region className="titlebar bg-zinc-800!">
            <ActionIcon.Group>
                <ActionIcon variant="transparent" color="gray" size="md" radius="xs" style={{ boxShadow: "none" }} onClick={() => appWindow.minimize()}>
                    <IconMinus size={15} stroke={1.5} />
                </ActionIcon>

                <ActionIcon variant="transparent" color="gray" size="md" radius="xs" style={{ boxShadow: "none" }} onClick={() => appWindow.toggleMaximize()}>
                    <IconSquare size={15} stroke={1.5} />
                </ActionIcon>

                <ActionIcon variant="transparent" color="gray" size="md" radius="xs" style={{ boxShadow: "none" }} onClick={() => appWindow.close()}>
                    <IconX size={20} stroke={1.5} />
                </ActionIcon>
            </ActionIcon.Group>
        </div>
    );
}

export default ToolBar;



