import { ActionIcon, Box, Button, ColorPicker, Modal, Popover, Textarea, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { useDisclosure } from "@mantine/hooks";
import { IconClipboardCheckFilled, IconPlus, IconSend, IconTag } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";



export default function New() {
    const [opened, { open, close }] = useDisclosure(false);
    const [colour, onChangeColour] = useState('rgba(47, 119, 150, 0.7)');


    const form = useForm({
        mode: 'uncontrolled',
        initialValues: {
            value: '',
            label: '',
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
        },

    });

    const handleSubmit = (values: typeof form.values) => {

        const clip = new FastClip(values.value, values.label, "📌", colour);
        console.log('Form submitted with:', clip);

        invoke('new_clip', { "clip": clip })
            .then((message) => console.log(message))
            .catch((error) => console.error(error));
        close();
        form.reset()
    };

    return (

        <>
            <Modal
                opened={opened}
                onClose={close}
                title="Add a new FastClip"
                fullScreen
                overlayProps={{
                    backgroundOpacity: 0.55,
                    blur: 3,
                }}
                transitionProps={{ transition: 'pop-bottom-right', duration: 150 }}
            >

                <form onSubmit={form.onSubmit(handleSubmit)} >

                    <TextInput
                        leftSection={<IconTag size={16} />}

                        withAsterisk
                        mt={10}
                        label="Label"
                        placeholder="This will be displayed"
                        key={form.key('label')}
                        {...form.getInputProps('label')}
                    />


                    <Textarea
                        mt={10}
                        leftSection={<IconClipboardCheckFilled size={16} />}
                        withAsterisk
                        label="What do you want to paste?"
                        resize="vertical"
                        placeholder="This will be pastable"
                        key={form.key('value')}
                        {...form.getInputProps('value')}
                    />




                    <Popover trapFocus position="bottom" withArrow shadow="md" >
                        <Popover.Target>
                            <Button autoContrast mt={10} fullWidth radius="xl" color={colour}>Select colour</Button>
                        </Popover.Target>
                        <Popover.Dropdown>
                            <ColorPicker format="rgba" value={colour} onChange={onChangeColour} />
                        </Popover.Dropdown>
                    </Popover>




                    <Box className='absolute  bottom-0 right-0'>
                        <ActionIcon variant="filled" color="blue" aria-label="Add" size="xl" m={10} type="submit">
                            <IconSend style={{ width: '70%', height: '70%' }} stroke={1.5} />
                        </ActionIcon>
                    </Box>


                </form>
            </Modal>


            <Box className='absolute  bottom-0 right-0'>
                <ActionIcon variant="filled" color="green" aria-label="Add" radius="xl" size="xl" m={10} onClick={open}>
                    <IconPlus style={{ width: '70%', height: '70%' }} stroke={1.5} />
                </ActionIcon>
            </Box>

        </>
    )
}
