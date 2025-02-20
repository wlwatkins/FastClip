import { ActionIcon, Box, Checkbox, Drawer } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { IconAdjustments } from '@tabler/icons-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";



export default function Settings() {
    const [opened, { open, close }] = useDisclosure(false);
    const [alwaysOnTop, setAlwaysOnTop] = useState(false);
    const [version, setVersion] = useState("");

    useEffect(() => {
      getVersion().then(setVersion);
    }, []);

    // const form = useForm({
    //     mode: 'uncontrolled',
    //     initialValues: {
    //         value: '',
    //         label: '',
    //     }, validate: {
    //         value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
    //         label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
    //     },

    // });


    const handleAlwaysOnTop = async () => {
        const newValue = !alwaysOnTop;
        setAlwaysOnTop(newValue); 
        console.log(newValue);
        console.log(alwaysOnTop);
        await getCurrentWindow().setAlwaysOnTop(newValue);
    };
    return (

        <>
            <Drawer opened={opened} onClose={close} title="Settings">
                <Checkbox
                    checked={alwaysOnTop}
                    onChange={handleAlwaysOnTop}
                    label="Keep on top"
                />



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
