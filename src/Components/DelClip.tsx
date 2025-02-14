import { ActionIcon, Box, Modal, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { useDisclosure } from "@mantine/hooks";
import { IconPlus, IconSend } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";




interface ItemProps {
    fast_clip: FastClip;
  }


  const Del: React.FC<ItemProps> = ({ fast_clip }) => {

    const [opened, { open, close }] = useDisclosure(true);


    const handleSubmit = () => {
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



     

                    {/* <TextInput
                        withAsterisk
                        mt={10}
                        label="Give it a label?"
                        placeholder="This will be displayed"
                        key={form.key('label')}
                        {...form.getInputProps('label')}
                    /> */}



                    <Box className='absolute  bottom-0 right-0'>
                        <ActionIcon variant="filled" color="blue" aria-label="Add" radius="xl" size="xl" m={10} type="submit">
                            <IconSend style={{ width: '70%', height: '70%' }} stroke={1.5} />
                        </ActionIcon>
                    </Box>


            </Modal>


            <Box className='absolute  bottom-0 right-0'>
                <ActionIcon variant="filled" color="red" aria-label="Add" radius="xl" size="xl" m={10} onClick={open}>
                    <IconPlus style={{ width: '70%', height: '70%' }} stroke={1.5} />
                </ActionIcon>
            </Box>

        </>

    )
}

export default Del;