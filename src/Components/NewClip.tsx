import { ActionIcon, Box, Button, Group, Modal, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { useDisclosure } from "@mantine/hooks";
import { IconPlus, IconSend } from '@tabler/icons-react';






export default function New() {
    const [opened, { open, close }] = useDisclosure(true);

    const form = useForm({
        mode: 'controlled',
        initialValues: {
            value: '',
            label: '',
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
        },

    });

    const handleSubmit = (values: typeof form.values) => {
        console.log('Form submitted with:', values);
        close();

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

                        withAsterisk
                        label="What do you want to paste?"
                        placeholder="This will be pastable"
                        key={form.key('value')}
                        {...form.getInputProps('value')}
                    />

                    <TextInput
                        withAsterisk
                        mt={10}
                        label="Give it a label?"
                        placeholder="This will be displayed"
                        key={form.key('label')}
                        {...form.getInputProps('label')}
                    />



                    <Box className='absolute  bottom-0 right-0'>
                        <ActionIcon variant="filled" color="blue" aria-label="Add" radius="xl" size="xl" m={10} type="submit">
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
