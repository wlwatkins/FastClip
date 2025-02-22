import { ActionIcon, Box, Button, Drawer, Stack, Switch } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { IconAdjustments } from '@tabler/icons-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open as load_file, save as save_file } from '@tauri-apps/plugin-dialog';
import { invoke } from "@tauri-apps/api/core";
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart';

export default function Settings() {
    const [opened, { open, close }] = useDisclosure(false);
    const [always_on_top, setAlwaysOnTop] = useState(false);
    const [start_on_boot, setStartOnBoot] = useState(false);
    const [version, setVersion] = useState("");

    useEffect(() => {
        // Fetch version on component mount
        getVersion().then(setVersion);

        // Check if the app should start on boot
        const fetchStartOnBoot = async () => {
            const isAppEnabled = await isEnabled();
            setStartOnBoot(isAppEnabled);
        };

        fetchStartOnBoot();
    }, []);


    const handleAlwaysOnTop = async () => {
        const newValue = !always_on_top;
        setAlwaysOnTop(newValue);
        await getCurrentWindow().setAlwaysOnTop(newValue);
    };

    const handleStartOnBoot = async () => {
        const newValue = !start_on_boot;
        setStartOnBoot(newValue);
        if (newValue) {
            await enable();
        } else {
            disable();
        }

        await getCurrentWindow().setAlwaysOnTop(newValue);
    };



    const handleLoad = async () => {
        const file = await load_file({
            multiple: false,
            directory: false,
            filters: [
                {
                    name: 'FastClip database',
                    extensions: ['csv'],
                },
                {
                    name: 'Any files',
                    extensions: ['*'],
                },
            ],
        });
        if (file) {
            invoke('load', { file }).then(close)
                .catch((error) => error(error));
        }

    };


    const handleSave = async () => {
        const file = await save_file({
            filters: [
                {
                    name: 'FastClip database',
                    extensions: ['csv'],
                }
            ],
        });
        if (file) {
            invoke('save', { file }).then(close)
                .catch((error) => error(error));
        }

    };


    return (

        <>
            <Drawer opened={opened} onClose={close} title="Settings">

                <Stack
                    align="stretch" justify="flex-start"
                    gap="md"
                >
                    <Switch
                        checked={always_on_top}
                        labelPosition="left"
                        onChange={handleAlwaysOnTop}
                        label="Keep window on top"
                    />
                    <Switch
                        checked={start_on_boot}
                        labelPosition="left"
                        onChange={handleStartOnBoot}
                        label="Start on system boot"
                    />
                    <Button.Group >
                        <Button variant="filled" size="xs" fullWidth onClick={handleLoad}>Load</Button>
                        <Button variant="filled" color="green" size="xs" fullWidth onClick={handleSave} >Export</Button>
                    </Button.Group>

                </Stack>


                <Box className='absolute  bottom-0 right-0'>
                    <p className="pr-2 text-gray-500">v{version}</p>
                </Box>



            </Drawer>


            <Box className='absolute  bottom-0 left-0'>
                <ActionIcon variant="light" color="gray" aria-label="Add" size="lg" m={10} onClick={open}>
                    <IconAdjustments style={{ width: '70%', height: '70%' }} stroke={1.5} />
                </ActionIcon>

            </Box>



        </>
    )
}
