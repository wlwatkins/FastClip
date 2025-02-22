import { ActionIcon, Box, ColorInput, Modal, NumberInput, Switch, Textarea, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { IconClipboardCheckFilled, IconDeviceFloppy, IconTag } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";


interface EditProps {
    fastClipRef: React.MutableRefObject<FastClip>;
    opened: boolean;
    open: () => void;
    close: () => void;
}

export default function Edit({ fastClipRef, opened, close }: EditProps) {
    const [clip, setClip] = useState(fastClipRef);
    const [colour, onChangeColour] = useState(fastClipRef.current.colour);
    const [clear_time_enable, setClearTimeEnable] = useState(fastClipRef.current.clear_time ? true : false);

    const form = useForm({
        mode: 'controlled',
        initialValues: {
            value: clip.current.value,
            label: clip.current.label,
            colour: clip.current.colour,
            clear_time: clear_time_enable ? clip.current.clear_time : 60
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
            clear_time: (value) => {
                if (value === undefined) return null; // Allow undefined
                if (typeof value !== 'number' || value < 1) {
                    return 'Must be a positive number';
                }
                return null; // No error
            }
        },
    });

    const handleSubmit = (values: typeof form.values) => {

        fastClipRef.current.value = values.value;
        fastClipRef.current.label = values.label;
        fastClipRef.current.colour = colour;
        fastClipRef.current.clear_time = clear_time_enable ? values.clear_time : undefined;
        setClip(fastClipRef);

        form.setInitialValues({
          value: fastClipRef.current.value,
          label: fastClipRef.current.label,
          colour: fastClipRef.current.colour,
          clear_time: fastClipRef.current.clear_time,
        });

        invoke('update_clip', {"clip": fastClipRef.current})
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


                    <Switch
                        mt={10}
                        mb={5}
                        size="xs"
                        checked={clear_time_enable}
                        labelPosition="left"
                        label="Clear clipboard after N secs"
                        onChange={(e) => setClearTimeEnable(e.target.checked)} // Extract boolean value
                     />
                    <NumberInput
                        disabled={!clear_time_enable}
                        min={10}
                        step={1}
                        value={form.values.clear_time} 
                        key={form.key('clear_time')}
                        {...form.getInputProps('clear_time')}
                    />

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
