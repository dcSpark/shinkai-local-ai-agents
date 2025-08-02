// TODO: Ignore it until we support qr code connection

// import { zodResolver } from '@hookform/resolvers/zod';
// import { QRSetupData } from '@shinkai_network/shinkai-message-ts/api/general/types';
// import { useSubmitRegistration } from '@shinkai_network/shinkai-node-state/v2/mutations/submitRegistation/useSubmitRegistration';
// import { generateMyEncryptionKeys } from '@shinkai_network/shinkai-node-state/v2/queries/getEncryptionKeys/index';
// import { useGetEncryptionKeys } from '@shinkai_network/shinkai-node-state/v2/queries/getEncryptionKeys/useGetEncryptionKeys';
// import {
//   Button,
//   ErrorMessage,
//   Form,
//   FormField,
//   TextField,
// } from '@shinkai_network/shinkai-ui';
// import { BrowserQRCodeReader } from '@zxing/browser';
// import { QrCode, Trash, Upload } from 'lucide-react';
// import React, { useEffect, useState } from 'react';
// import { useForm } from 'react-hook-form';
// import { useNavigate } from 'react-router';
// import { z } from 'zod';

// import { Auth, useAuth } from '../store/auth';

// const formSchema = z.object({
//   registration_code: z.string().min(5),
//   registration_name: z.string().min(5),
//   permission_type: z.enum(['admin']),
//   identity_type: z.enum(['device']),
//   profile: z.enum(['main']),
//   node_address: z.string().url(),
//   shinkai_identity: z.string().min(11),
//   node_encryption_pk: z.string(),
//   node_signature_pk: z.string(),
//   profile_encryption_pk: z.string().min(5),
//   profile_identity_pk: z.string().min(5),
//   profile_encryption_sk: z.string().min(5),
//   profile_identity_sk: z.string().min(5),
// });

// type FormType = z.infer<typeof formSchema>;

// type AddNodeDataFromQr = Pick<
//   FormType,
//   | 'registration_code'
//   | 'node_address'
//   | 'shinkai_identity'
//   | 'node_encryption_pk'
//   | 'node_signature_pk'
// >;

// export const ConnectMethodQrCodePage = () => {
//   const setAuth = useAuth((state) => state.setAuth);
//   const navigate = useNavigate();
//   const { encryptionKeys, isLoading: isLoadingEncryptionKeys } =
//     useGetEncryptionKeys();
//   const form = useForm<FormType>({
//     resolver: zodResolver(formSchema),
//     defaultValues: {
//       registration_code: '',
//       registration_name: `device_${Date.now()}`,
//       permission_type: 'admin',
//       identity_type: 'device',
//       profile: 'main',
//       node_address: '',
//       shinkai_identity: '',
//       node_encryption_pk: '',
//       node_signature_pk: '',
//       ...encryptionKeys,
//     },
//   });
//   const [qrImageFile, setQRImageFile] = useState<File | null>(null);
//   const [qrImageUrl, setQRImageUrl] = useState<string | null>(null);

//   const {
//     isPending,
//     mutateAsync: submitRegistration,
//     isError: isSubmitError,
//     error: submitError,
//   } = useSubmitRegistration({
//     onSuccess: (response) => {
//       if (response) {
//         const values = form.getValues();
//         const authData: Auth = {
//           ...values,
//           node_signature_pk: response.identity_public_key ?? '',
//           node_encryption_pk: response.encryption_public_key ?? '',
//           api_v2_key: '',
//         };
//         authSuccess(authData);
//       } else {
//         throw new Error('Failed to submit registration');
//       }
//     },
//   });

//   const onQRImageSelected: React.ChangeEventHandler<HTMLInputElement> = async (
//     event,
//   ): Promise<void> => {
//     if (!event.target.files || !event.target.files[0]) {
//       return;
//     }
//     const file = event.target.files[0];
//     const qrImageUrl = URL.createObjectURL(file);
//     const codeReader = new BrowserQRCodeReader();
//     const resultImage = await codeReader.decodeFromImageUrl(qrImageUrl);
//     const jsonString = resultImage.getText();
//     const parsedQrData: QRSetupData = JSON.parse(jsonString);
//     const nodeDataFromQr = getValuesFromQr(parsedQrData);
//     setQRImageFile(file);
//     form.reset((prev) => ({ ...prev, ...nodeDataFromQr }));
//   };

//   const getValuesFromQr = (qrData: QRSetupData): AddNodeDataFromQr => {
//     return {
//       ...qrData,
//     };
//   };

//   const connect = (values: FormType) => {
//     submitRegistration({
//       ...values,
//       registration_code: values.registration_code ?? '',
//       node_encryption_pk: values.node_encryption_pk ?? '',
//     });
//   };

//   const authSuccess = (setupData: Auth) => {
//     setAuth(setupData);
//     navigate('/inboxes');
//   };

//   useEffect(() => {
//     generateMyEncryptionKeys().then((encryptionKeys) => {
//       form.reset((prevInitialValues) => ({
//         ...prevInitialValues,
//         ...encryptionKeys,
//       }));
//     });
//   }, [form]);

//   useEffect(() => {
//     if (qrImageFile) {
//       const qrImageUrl = URL.createObjectURL(qrImageFile);
//       setQRImageUrl(qrImageUrl);
//     } else {
//       setQRImageUrl('');
//     }
//   }, [qrImageFile]);

//   const removeQRFile = () => {
//     setQRImageFile(null);
//     form.reset();
//   };

//   return (
//     <div className="flex h-full flex-col space-y-3">
//       <div className="flex flex-col space-y-2">
//         <div className="flex flex-row items-center space-x-2">
//           <h1 className="text-left text-2xl font-semibold">QR Code</h1>
//           <QrCode />
//         </div>
//         <p>Use a QR code to connect</p>
//       </div>
//       <Form {...form}>
//         <form
//           className="flex h-full flex-col justify-between space-y-2"
//           onSubmit={form.handleSubmit(connect)}
//         >
//           <div className="flex flex-col space-y-2">
//             <div className="flex flex-col space-y-1">
//               <div className="flex items-center justify-center">
//                 <div className="flex h-[100px] w-full flex-col items-center justify-center rounded-lg border-2 border-dashed">
//                   {qrImageFile && qrImageUrl ? (
//                     <div className="flex flex-row items-center justify-center space-x-3">
//                       <div className="flex flex-row items-center">
//                         <QrCode className="mr-1 h-4 w-4 space-x-1" />
//                         <span className="font-semibold">
//                           {qrImageFile.name}
//                         </span>
//                       </div>
//                       <div className="relative">
//                         <img
//                           alt="qr connection data"
//                           className="h-[80px]"
//                           src={qrImageUrl}
//                         />
//                         <Button
//                           className="absolute right-1 top-1 h-6 w-6"
//                           onClick={() => removeQRFile()}
//                           size="icon"
//                           type="button"
//                         >
//                           <Trash className="h-4 w-4" />
//                         </Button>
//                       </div>
//                     </div>
//                   ) : (
//                     <label
//                       className="flex h-[100px] w-full cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-gray-100 bg-gray-400"
//                       htmlFor="dropzone-file"
//                     >
//                       <div className="flex flex-col items-center justify-center space-y-1">
//                         <div>
//                           <Upload className="h-4 w-4" />
//                         </div>
//                         <p className="text-sm text-white">
//                           Click to upload or drag and drop
//                         </p>
//                         <p className="text-text-secondary text-xs">JPG | PNG</p>
//                       </div>
//                       <input
//                         accept="image/png, image/jpeg"
//                         alt="shinaki conection file input"
//                         className="hidden"
//                         id="dropzone-file"
//                         onChange={(event) => onQRImageSelected(event)}
//                         type="file"
//                       />
//                     </label>
//                   )}
//                 </div>
//               </div>
//             </div>
//             {qrImageFile && (
//               <>
//                 <FormField
//                   control={form.control}
//                   name="registration_name"
//                   render={({ field }) => (
//                     <TextField field={field} label="Registration Name" />
//                   )}
//                 />

//                 <FormField
//                   control={form.control}
//                   name="node_address"
//                   render={({ field }) => (
//                     <TextField field={field} label="Node Address" />
//                   )}
//                 />

//                 <FormField
//                   control={form.control}
//                   name="shinkai_identity"
//                   render={({ field }) => (
//                     <TextField field={field} label="Identity Name" />
//                   )}
//                 />
//               </>
//             )}

//             {isSubmitError && <ErrorMessage message={submitError?.message} />}
//           </div>
//           <Button
//             className="w-full"
//             disabled={isPending || isLoadingEncryptionKeys}
//             isLoading={isPending}
//             type="submit"
//           >
//             Connect
//           </Button>
//         </form>
//       </Form>
//     </div>
//   );
// };
