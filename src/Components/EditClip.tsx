import { ActionIcon, Box, Modal, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { useDisclosure } from "@mantine/hooks";
import { IconAt, IconClipboardCheckFilled, IconPlus, IconSend, IconTag } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";


interface EditProps {
    fast_clip: FastClip;
    opened: boolean;
    open: () => void;
    close: () => void;
}

export default function Edit({ fast_clip, opened, open, close }: EditProps) {


    const form = useForm({
        mode: 'uncontrolled',
        initialValues: {
            value: fast_clip.value,
            label: fast_clip.label,
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
        },
    });

    const handleSubmit = (values: typeof form.values) => {
        fast_clip.value = values.value;
        fast_clip.label = values.label;
        
        console.log('Form submitted with:', fast_clip);

        invoke('update_clip', { fast_clip })
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
                title="Edit a FastClip"
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


                    <TextInput
                        leftSection={<IconClipboardCheckFilled size={16} />}
                        withAsterisk
                        label="What do you want to paste?"
                        placeholder="This will be pastable"
                        key={form.key('value')}
                        {...form.getInputProps('value')}
                    />



                    <Box className='absolute  bottom-0 right-0'>
                        <ActionIcon variant="filled" color="blue" aria-label="Add" radius="xl" size="xl" m={10} type="submit">
                            <IconSend style={{ width: '70%', height: '70%' }} stroke={1.5} />
                        </ActionIcon>
                    </Box>


                </form>
            </Modal>



        </>
    )
}
