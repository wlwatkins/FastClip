import { ActionIcon, Box, Button, ColorPicker, Modal, Popover, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { IconClipboardCheckFilled, IconDeviceFloppy, IconTag } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";


interface EditProps {
    fast_clip: FastClip;

    opened: boolean;
    open: () => void;
    close: () => void;
}

export default function Edit({ fast_clip, opened, close }: EditProps) {
    const [clip, setClip] = useState(fast_clip);
    const [colour, onChangeColour] = useState('rgba(47, 119, 150, 0.7)');

    const form = useForm({
        mode: 'uncontrolled',
        initialValues: {
            value: clip.value,
            label: clip.label,
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
        },
    });

    useEffect(() => {
        form.setValues({
          value: clip.value,
          label: clip.label,
        });
      }, [clip]); // Runs whenever `clip` changes

    const handleSubmit = (values: typeof form.values) => {

        fast_clip.value = values.value;
        fast_clip.label = values.label;
        fast_clip.colour = colour;
        setClip(fast_clip);

        console.log('Form submitted with:', {"clip": fast_clip});

        invoke('update_clip', {"clip": fast_clip})
            .then((message) => console.log(message))
            .catch((error) => console.error(error));
        close();
        form.reset()
    };

    return (
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
                    mt={10}
                    leftSection={<IconClipboardCheckFilled size={16} />}
                    withAsterisk
                    label="What do you want to paste?"
                    placeholder="This will be pastable"
                    key={form.key('value')}
                    {...form.getInputProps('value')}
                />



                <Popover  trapFocus position="bottom" withArrow shadow="md" >
                    <Popover.Target>
                        <Button autoContrast mt={10} fullWidth radius="xl" color={colour}>Select colour</Button>
                    </Popover.Target>
                    <Popover.Dropdown>
                        <ColorPicker format="rgba" value={colour} onChange={onChangeColour} />
                    </Popover.Dropdown>
                </Popover>





                <Box className='absolute  bottom-0 right-0'>
                    <ActionIcon variant="filled" color="pink" aria-label="Add" size="xl" m={10} type="submit">
                        <IconDeviceFloppy style={{ width: '70%', height: '70%' }} stroke={1.5} />
                    </ActionIcon>
                </Box>


            </form>
        </Modal>
    )
}
