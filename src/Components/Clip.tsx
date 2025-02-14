import { ActionIcon, Button, Grid } from "@mantine/core";
import { IconEdit, IconTrashX, IconEyeOff } from '@tabler/icons-react';
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import FastClip from "../Classes/FastClip";

interface ItemProps {
  fast_clip: FastClip;
}

const Clip: React.FC<ItemProps> = ({ fast_clip }) => {



  const handlePutInClipBoard = async () => {
    try {
      await writeText(fast_clip.value);
      console.log('Text written successfully');
    } catch (error) {
      console.error('Error writing text:', error);
    }
  };




  return (


    <Grid w="90%">
      <Grid.Col span="auto">
        <Button fullWidth variant="filled" color="gray" radius="md" onClick={handlePutInClipBoard}>{fast_clip.label}</Button>
      </Grid.Col>
      <Grid.Col span="content" >
        <ActionIcon.Group >

          <ActionIcon variant="light" size="lg" aria-label="Gallery">
            <IconEdit size={20} stroke={1.5} />
          </ActionIcon>

          <ActionIcon variant="light" color="red" size="lg" aria-label="Settings"  onClick={handlePutInClipBoard}>
            <IconTrashX size={20} stroke={1.5} />
          </ActionIcon>

          <ActionIcon variant="light" color="gray" size="lg" aria-label="Likes">
            <IconEyeOff size={20} stroke={1.5} />
          </ActionIcon>
        </ActionIcon.Group>
      </Grid.Col>
    </Grid>

  )
}
export default Clip;