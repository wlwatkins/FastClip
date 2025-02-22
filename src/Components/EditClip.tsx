import { ActionIcon, Box, ColorInput, Modal, Textarea, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { IconClipboardCheckFilled, IconDeviceFloppy, IconTag } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { info } from '@tauri-apps/plugin-log';


interface EditProps {
    fastClipRef: React.MutableRefObject<FastClip>;
    opened: boolean;
    open: () => void;
    close: () => void;
}

export default function Edit({ fastClipRef, opened, close }: EditProps) {
    const [clip, setClip] = useState(fastClipRef);
    const [colour, onChangeColour] = useState('rgba(47, 119, 150, 0.7)');

    const form = useForm({
        mode: 'controlled',
        initialValues: {
            value: clip.current.value,
            label: clip.current.label,
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
        },
    });

    const handleSubmit = (values: typeof form.values) => {

        fastClipRef.current.value = values.value;
        fastClipRef.current.label = values.label;
        fastClipRef.current.colour = colour;
        setClip(fastClipRef);

        form.setInitialValues({
          value: fastClipRef.current.value,
          label: fastClipRef.current.label,
        });

        invoke('update_clip', {"clip": fastClipRef})
            .then((message: any) => info(message))
            .catch((error) => error(error));
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


                <Textarea
                    mt={10}
                    minRows={2}
                    maxRows={4}
                    leftSection={<IconClipboardCheckFilled size={16} />}
                    withAsterisk
                    label="What do you want to paste?"
                    resize="vertical"
                    placeholder="This will be pastable"
                    key={form.key('value')}
                    {...form.getInputProps('value')}
                />



                {/* <Popover  trapFocus position="bottom" withArrow shadow="md" >
                    <Popover.Target>
                        <Button autoContrast mt={10} fullWidth radius="xl" color={colour}>Select colour</Button>
                    </Popover.Target>
                    <Popover.Dropdown>
                        <ColorPicker format="rgba" value={colour} onChange={onChangeColour} />
                    </Popover.Dropdown>
                </Popover> */}


                <ColorInput
                    mt={10} 
                    label="Colour"
                    swatchesPerRow={12}
                    disallowInput
                    format="hex"
                    onChange={onChangeColour}
                    defaultValue={colour}
                    withPicker={false}
                    swatches={['#000814', '#001D3D', '#003566', '#FFC300', '#FFD60A']}
                />


                <Box className='absolute  bottom-0 right-0'>
                    <ActionIcon variant="filled" color="pink" aria-label="Add" size="xl" m={10} type="submit">
                        <IconDeviceFloppy style={{ width: '70%', height: '70%' }} stroke={1.5} />
                    </ActionIcon>
                </Box>


            </form>
        </Modal>
    )
}
